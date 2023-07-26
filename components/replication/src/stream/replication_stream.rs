use crate::stream::replication_range::RangeAppendContext;
use crate::stream::replication_range::ReplicationRange;

use super::cache::HotCache;
use super::FetchDataset;
use super::Stream;
use client::client::Client;
use itertools::Itertools;
use local_sync::{mpsc, oneshot};
use log::{error, info, trace, warn};
use model::error::EsError;
use model::RecordBatch;
use protocol::rpc::header::ErrorCode;
use std::cell::OnceCell;
use std::cell::RefCell;
use std::cmp::min;
use std::collections::BTreeMap;
use std::ops::Bound::Included;
use std::rc::{Rc, Weak};
use std::time::Instant;
use tokio::sync::broadcast;
use tokio::time::{sleep, Duration};

pub(crate) struct ReplicationStream<R, C>
where
    R: ReplicationRange<C> + 'static,
    C: Client + 'static,
{
    log_ident: String,
    weak_self: RefCell<Weak<Self>>,
    id: i64,
    epoch: u64,
    ranges: RefCell<BTreeMap<u64, Rc<R>>>,
    client: Weak<C>,
    next_offset: RefCell<u64>,
    last_range: RefCell<Option<Rc<R>>>,
    /// #append send StreamAppendRequest to tx.
    append_requests_tx: mpsc::bounded::Tx<StreamAppendRequest>,
    /// send by range ack / delay retry to trigger append task loop next round.
    append_tasks_tx: mpsc::unbounded::Tx<()>,
    // send when stream close.
    shutdown_signal_tx: broadcast::Sender<()>,
    // stream closed mark.
    closed: Rc<RefCell<bool>>,
    cache: Rc<HotCache>,
}

impl<R, C> ReplicationStream<R, C>
where
    R: ReplicationRange<C> + 'static,
    C: Client + 'static,
{
    pub(crate) fn new(id: i64, epoch: u64, client: Weak<C>, cache: Rc<HotCache>) -> Rc<Self> {
        let (append_requests_tx, append_requests_rx) = mpsc::bounded::channel(1024);
        let (append_tasks_tx, append_tasks_rx) = mpsc::unbounded::channel();
        let (shutdown_signal_tx, shutdown_signal_rx) = broadcast::channel(1);
        let this = Rc::new(Self {
            log_ident: format!("Stream[{id}] "),
            weak_self: RefCell::new(Weak::new()),
            id,
            epoch,
            ranges: RefCell::new(BTreeMap::new()),
            client,
            next_offset: RefCell::new(0),
            last_range: RefCell::new(Option::None),
            append_requests_tx,
            append_tasks_tx,
            shutdown_signal_tx,
            closed: Rc::new(RefCell::new(false)),
            cache,
        });

        *(this.weak_self.borrow_mut()) = Rc::downgrade(&this);

        let weak_this = this.weak_self.borrow().clone();
        let closed = this.closed.clone();
        tokio_uring::spawn(async move {
            Self::append_task(
                weak_this,
                append_requests_rx,
                append_tasks_rx,
                shutdown_signal_rx,
                closed,
            )
            .await
        });

        this
    }

    async fn new_range(&self, range_index: i32, start_offset: u64) -> Result<(), EsError> {
        if let Some(client) = self.client.upgrade() {
            let range_metadata =
                R::create(client, self.id, self.epoch, range_index, start_offset).await?;
            let weak_this = self.weak_self.borrow().clone();
            let range = R::new(
                range_metadata,
                true,
                Box::new(move || {
                    if let Some(stream) = weak_this.upgrade() {
                        stream.try_ack();
                    }
                }),
                self.client.clone(),
                self.cache.clone(),
            );
            info!("{}Create new range: {:?}", range.metadata(), self.log_ident);
            self.ranges.borrow_mut().insert(start_offset, range.clone());
            *self.last_range.borrow_mut() = Some(range);
            Ok(())
        } else {
            Err(EsError::new(
                ErrorCode::UNEXPECTED,
                "new range fail, client is drop",
            ))
        }
    }

    pub(crate) fn try_ack(&self) {
        self.trigger_append_task();
    }

    pub(crate) fn trigger_append_task(&self) {
        let _ = self.append_tasks_tx.send(());
    }

    async fn append_task(
        stream: Weak<Self>,
        mut append_requests_rx: mpsc::bounded::Rx<StreamAppendRequest>,
        mut append_tasks_rx: mpsc::unbounded::Rx<()>,
        mut shutdown_signal_rx: broadcast::Receiver<()>,
        closed: Rc<RefCell<bool>>,
    ) {
        let stream_option = stream.upgrade();
        if stream_option.is_none() {
            warn!("Stream is already released, then directly exit append task");
            return;
        }
        let stream = stream_option.expect("stream id cannot be none");
        let log_ident = &stream.log_ident;
        let mut inflight: BTreeMap<u64, Rc<StreamAppendRequest>> = BTreeMap::new();
        let mut next_append_start_offset: u64 = 0;

        loop {
            tokio::select! {
                Some(append_request) = append_requests_rx.recv() => {
                    inflight.insert(append_request.base_offset(), Rc::new(append_request));
                }
                Some(_) = append_tasks_rx.recv() => {
                    // usually send by range ack / delay retry
                }
                _ = shutdown_signal_rx.recv() => {
                    let inflight_count = inflight.len();
                    info!("{}Receive shutdown signal, then quick fail {inflight_count} inflight requests with AlreadyClosed err.", log_ident);
                    for (_, append_request) in inflight.iter() {
                        append_request.fail(EsError::new(ErrorCode::STREAM_ALREADY_CLOSED, "stream is closed"));
                    }
                    break;
                }
            }
            if *closed.borrow() {
                let inflight_count = inflight.len();
                info!("{}Detect closed mark, then quick fail {inflight_count} inflight requests with AlreadyClosed err.", log_ident);
                for (_, append_request) in inflight.iter() {
                    append_request.fail(EsError::new(
                        ErrorCode::STREAM_ALREADY_CLOSED,
                        "stream is closed",
                    ));
                }
                break;
            }

            // 1. get writable range.
            let last_range = stream.last_range.borrow().as_ref().cloned();
            let last_writable_range = match last_range {
                Some(last_range) => {
                    let range_index = last_range.metadata().index();
                    if !last_range.is_writable() {
                        info!("{}The last range[{range_index}] is not writable, try create a new range.", log_ident);
                        // if last range is not writable, try to seal it and create a new range and retry append in next round.
                        match last_range.seal().await {
                            Ok(end_offset) => {
                                info!("{}Seal not writable last range[{range_index}] with end_offset={end_offset}.", log_ident);
                                // rewind back next append start offset and try append to new writable range in next round.
                                next_append_start_offset = end_offset;
                                if let Err(e) = stream.new_range(range_index + 1, end_offset).await
                                {
                                    error!(
                                        "{}Try create a new range fail, retry later, err[{e}]",
                                        log_ident
                                    );
                                    // delay retry to avoid busy loop
                                    sleep(Duration::from_millis(1000)).await;
                                }
                            }
                            Err(_) => {
                                // delay retry to avoid busy loop
                                sleep(Duration::from_millis(1000)).await;
                            }
                        }
                        stream.trigger_append_task();
                        continue;
                    }
                    last_range
                }
                None => {
                    info!(
                        "{}The stream don't have any range, then try new a range.",
                        log_ident
                    );
                    if let Err(e) = stream.new_range(0, 0).await {
                        error!(
                            "{}New a range from absent fail, retry later, err[{e}]",
                            log_ident
                        );
                        // delay retry to avoid busy loop
                        sleep(Duration::from_millis(1000)).await;
                    }
                    stream.trigger_append_task();
                    continue;
                }
            };
            if !inflight.is_empty() {
                let range_index = last_writable_range.metadata().index();
                // 2. ack success append request, and remove them from inflight.
                let confirm_offset = last_writable_range.confirm_offset();
                let mut ack_count = 0;
                for (base_offset, append_request) in inflight.iter() {
                    if *base_offset < confirm_offset {
                        // if base offset is less than confirm offset, it means append request is already success.
                        append_request.success();
                        ack_count += 1;
                        trace!("{}Ack append request with base_offset={base_offset}, confirm_offset={confirm_offset}", log_ident);
                    }
                }
                for _ in 0..ack_count {
                    inflight.pop_first();
                }

                // 3. try append request which base_offset >= next_append_start_offset.
                let mut cursor = inflight.lower_bound(Included(&next_append_start_offset));
                while let Some((base_offset, append_request)) = cursor.key_value() {
                    last_writable_range.append(
                        &append_request.record_batch,
                        RangeAppendContext::new(*base_offset),
                    );
                    trace!(
                        "{}Try append record[{base_offset}] to range[{range_index}]",
                        log_ident
                    );
                    next_append_start_offset = base_offset + append_request.count() as u64;
                    cursor.move_next();
                }
            }
        }
    }
}

impl<R, C> Stream for ReplicationStream<R, C>
where
    R: ReplicationRange<C> + 'static,
    C: Client + 'static,
{
    async fn open(&self) -> Result<(), EsError> {
        info!("{}Opening...", self.log_ident);
        let client = self.client.upgrade().ok_or(EsError::new(
            ErrorCode::UNEXPECTED,
            "open fail, client is dropped",
        ))?;
        // 1. load all ranges
        client
            .list_ranges(model::ListRangeCriteria::new(None, Some(self.id as u64)))
            .await
            .map_err(|e| {
                error!(
                    "{}Failed to list ranges from placement-driver: {e}",
                    self.log_ident
                );
                e
            })?
            .into_iter()
            // skip old empty range when two range has the same start offset
            .sorted_by(|a, b| Ord::cmp(&a.index(), &b.index()))
            .for_each(|range| {
                let this = self.weak_self.borrow().clone();
                self.ranges.borrow_mut().insert(
                    range.start(),
                    R::new(
                        range,
                        false,
                        Box::new(move || {
                            if let Some(stream) = this.upgrade() {
                                stream.try_ack();
                            }
                        }),
                        self.client.clone(),
                        self.cache.clone(),
                    ),
                );
            });
        // 2. seal the last range
        let last_range = self
            .ranges
            .borrow_mut()
            .last_entry()
            .map(|e| e.get().clone());
        if let Some(last_range) = last_range {
            *(self.last_range.borrow_mut()) = Option::Some(last_range.clone());
            let range_index = last_range.metadata().index();
            match last_range.seal().await {
                Ok(confirm_offset) => {
                    // 3. set stream next offset to the exclusive end of the last range
                    *self.next_offset.borrow_mut() = confirm_offset;
                }
                Err(e) => {
                    error!("{}Failed to seal range[{range_index}], {e}", self.log_ident);
                    return Err(e);
                }
            }
        }
        let range_count = self.ranges.borrow().len();
        let start_offset = self.start_offset();
        let next_offset = self.next_offset();
        info!("{}Opened with range_count={range_count} start_offset={start_offset} next_offset={next_offset}", self.log_ident);
        Ok(())
    }

    /// Close the stream.
    /// 1. send stop signal to append task.
    /// 2. await append task to stop.
    /// 3. close all ranges.
    async fn close(&self) {
        info!("{}Closing...", self.log_ident);
        *self.closed.borrow_mut() = true;
        let _ = self.shutdown_signal_tx.send(());
        // TODO: await append task to stop.
        let last_range = self.last_range.borrow().as_ref().cloned();
        if let Some(range) = last_range {
            let _ = range.seal().await;
        }
        info!("{}Closed...", self.log_ident);
    }

    fn start_offset(&self) -> u64 {
        self.ranges
            .borrow()
            .first_key_value()
            .map(|(k, _)| *k)
            .unwrap_or(0)
    }

    fn confirm_offset(&self) -> u64 {
        self.last_range
            .borrow()
            .as_ref()
            .map_or(0, |r| r.confirm_offset())
    }

    /// next record offset to be appended.
    fn next_offset(&self) -> u64 {
        *self.next_offset.borrow()
    }

    async fn append(&self, record_batch: RecordBatch) -> Result<u64, EsError> {
        let start_timestamp = Instant::now();
        if *self.closed.borrow() {
            warn!("{}Keep append to a closed stream.", self.log_ident);
            return Err(EsError::new(
                ErrorCode::STREAM_ALREADY_CLOSED,
                "stream is closed",
            ));
        }
        let base_offset = *self.next_offset.borrow();
        let count = record_batch.last_offset_delta();
        *self.next_offset.borrow_mut() = base_offset + count as u64;

        let (append_tx, append_rx) = oneshot::channel::<Result<(), EsError>>();
        // trigger background append task to handle the append request.
        if self
            .append_requests_tx
            .send(StreamAppendRequest::new(
                base_offset,
                record_batch,
                append_tx,
            ))
            .await
            .is_err()
        {
            warn!("{}Send to append request channel fail.", self.log_ident);
            return Err(EsError::new(
                ErrorCode::STREAM_ALREADY_CLOSED,
                "stream is closed",
            ));
        }
        // await append result.
        match append_rx.await {
            Ok(result) => {
                trace!(
                    "{}Append new record with base_offset={base_offset} count={count} cost {}us",
                    self.log_ident,
                    start_timestamp.elapsed().as_micros()
                );
                result.map(|_| base_offset)
            }
            Err(_) => Err(EsError::new(
                ErrorCode::STREAM_ALREADY_CLOSED,
                "stream is closed",
            )),
        }
    }

    async fn fetch(
        &self,
        start_offset: u64,
        end_offset: u64,
        batch_max_bytes: u32,
    ) -> Result<FetchDataset, EsError> {
        trace!(
            "{}Fetch [{start_offset}, {end_offset}) with batch_max_bytes={batch_max_bytes}",
            self.log_ident
        );
        if start_offset == end_offset {
            return Ok(FetchDataset::Partial(vec![]));
        }
        let last_range = match self.last_range.borrow().as_ref() {
            Some(range) => range.clone(),
            None => {
                return Err(EsError::new(
                    ErrorCode::OFFSET_OUT_OF_RANGE_BOUNDS,
                    "fetch out of range",
                ))
            }
        };
        // Fetch range is out of stream range.
        if last_range.confirm_offset() < end_offset {
            return Err(EsError::new(
                ErrorCode::OFFSET_OUT_OF_RANGE_BOUNDS,
                "fetch out of range",
            ));
        }
        // Fast path: if fetch range is in the last range, then fetch from it.
        if last_range.start_offset() <= start_offset {
            return last_range
                .fetch(start_offset, end_offset, batch_max_bytes)
                .await;
        }
        // Slow path
        // 1. Find the *first* range which match the start_offset.
        // 2. Fetch the range.
        // 3. The invoker should loop invoke fetch util the Dataset fullfil the need.
        let range = self
            .ranges
            .borrow()
            .upper_bound(Included(&start_offset))
            .value()
            .cloned();
        if let Some(range) = range {
            if range.start_offset() > start_offset {
                return Err(EsError::new(
                    ErrorCode::OFFSET_OUT_OF_RANGE_BOUNDS,
                    "fetch out of range",
                ));
            }
            let dataset = match range
                .fetch(
                    start_offset,
                    min(end_offset, range.confirm_offset()),
                    batch_max_bytes,
                )
                .await?
            {
                // Cause of only fetch one range in a time, so the dataset is partial
                FetchDataset::Full(blocks) => FetchDataset::Partial(blocks),
                FetchDataset::Mixin(blocks, objects) => FetchDataset::Mixin(blocks, objects),
                _ => {
                    error!(
                        "{}Fetch dataset should not be Partial or Overflow",
                        self.log_ident
                    );
                    return Err(EsError::new(
                        ErrorCode::UNEXPECTED,
                        "fetch dataset should not be Partial or Overflow",
                    ));
                }
            };
            Ok(dataset)
        } else {
            Err(EsError::new(
                ErrorCode::OFFSET_OUT_OF_RANGE_BOUNDS,
                "fetch out of range",
            ))
        }
    }

    async fn trim(&self, _new_start_offset: u64) -> Result<(), EsError> {
        todo!()
    }
}

struct StreamAppendRequest {
    base_offset: u64,
    record_batch: RecordBatch,
    append_tx: RefCell<OnceCell<oneshot::Sender<Result<(), EsError>>>>,
}

impl StreamAppendRequest {
    pub fn new(
        base_offset: u64,
        record_batch: RecordBatch,
        append_tx: oneshot::Sender<Result<(), EsError>>,
    ) -> Self {
        let append_tx_cell = OnceCell::new();
        let _ = append_tx_cell.set(append_tx);
        Self {
            base_offset,
            record_batch,
            append_tx: RefCell::new(append_tx_cell),
        }
    }

    pub fn base_offset(&self) -> u64 {
        self.base_offset
    }

    pub fn count(&self) -> u32 {
        self.record_batch.last_offset_delta() as u32
    }

    pub fn success(&self) {
        if let Some(append_tx) = self.append_tx.borrow_mut().take() {
            let _ = append_tx.send(Ok(()));
        }
    }

    pub fn fail(&self, err: EsError) {
        if let Some(append_tx) = self.append_tx.borrow_mut().take() {
            let _ = append_tx.send(Err(err));
        }
    }
}
