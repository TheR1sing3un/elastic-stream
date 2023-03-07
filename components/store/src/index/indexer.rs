use std::{ffi::CString, fs, io::Cursor, path::Path, rc::Rc};

use bytes::{Buf, BufMut, BytesMut};
use model::range::{Range, StreamRange};
use rocksdb::{
    BlockBasedOptions, ColumnFamilyDescriptor, DBCompressionType, Options, ReadOptions,
    WriteOptions, DB,
};
use slog::{error, info, Logger};

use crate::error::StoreError;

use super::MinOffset;

const INDEX_COLUMN_FAMILY: &str = "index";
const METADATA_COLUMN_FAMILY: &str = "metadata";

/// Key-value in metadata column family, flagging WAL offset, prior to which primary index are already built.
const WAL_CHECKPOINT: &str = "wal_checkpoint";

/// Representation of a range in metadata column family: {range-prefix: 1B}{stream_id:8B}{begin:8B} --> {status: 1B}[{end: 8B}].
///
/// If a range is sealed, its status is changed from `0` to `1` and logical `end` will be appended.
const RANGE_PREFIX: u8 = b'r';

pub(crate) struct Indexer {
    log: Logger,
    db: DB,
    write_opts: WriteOptions,
}

impl Indexer {
    fn build_index_column_family_options(
        log: Logger,
        min_offset: Rc<dyn MinOffset>,
    ) -> Result<Options, StoreError> {
        let mut index_cf_opts = Options::default();
        index_cf_opts.enable_statistics();
        index_cf_opts.set_write_buffer_size(128 * 1024 * 1024);
        index_cf_opts.set_max_write_buffer_number(4);
        index_cf_opts.set_compression_type(DBCompressionType::None);
        {
            // 128MiB block cache
            let cache = rocksdb::Cache::new_lru_cache(128 << 20)
                .map_err(|e| StoreError::RocksDB(e.into_string()))?;
            let mut table_opts = BlockBasedOptions::default();
            table_opts.set_block_cache(&cache);
            table_opts.set_block_size(128 << 10);
            index_cf_opts.set_block_based_table_factory(&table_opts);
        }

        let compaction_filter_name = CString::new("index-compaction-filter")
            .map_err(|e| StoreError::Internal("Failed to create CString".to_owned()))?;

        let index_compaction_filter_factory = super::compaction::IndexCompactionFilterFactory::new(
            log.clone(),
            compaction_filter_name,
            min_offset,
        );

        index_cf_opts.set_compaction_filter_factory(index_compaction_filter_factory);
        Ok(index_cf_opts)
    }

    pub(crate) fn new(
        log: Logger,
        path: &str,
        min_offset: Rc<dyn MinOffset>,
    ) -> Result<Self, StoreError> {
        let path = Path::new(path);
        if !path.exists() {
            info!(log, "Create directory: {:?}", path);
            fs::create_dir_all(path).map_err(|e| StoreError::IO(e))?;
        }

        let index_cf_opts = Self::build_index_column_family_options(log.clone(), min_offset)?;
        let index_cf = ColumnFamilyDescriptor::new(INDEX_COLUMN_FAMILY, index_cf_opts);

        let mut metadata_cf_opts = Options::default();
        metadata_cf_opts.enable_statistics();
        metadata_cf_opts.create_if_missing(true);
        metadata_cf_opts.optimize_for_point_lookup(16 << 20);
        let metadata_cf = ColumnFamilyDescriptor::new(METADATA_COLUMN_FAMILY, metadata_cf_opts);

        let mut db_opts = Options::default();
        db_opts.set_atomic_flush(true);
        db_opts.create_if_missing(true);
        db_opts.create_missing_column_families(true);
        db_opts.enable_statistics();
        db_opts.set_stats_dump_period_sec(10);
        db_opts.set_stats_persist_period_sec(10);
        // Threshold of all memtable across column families added in size
        db_opts.set_db_write_buffer_size(1024 * 1024 * 1024);
        db_opts.set_max_background_jobs(2);

        let mut write_opts = WriteOptions::default();
        write_opts.disable_wal(true);
        write_opts.set_sync(false);

        let db = DB::open_cf_descriptors(&db_opts, path, vec![index_cf, metadata_cf])
            .map_err(|e| StoreError::RocksDB(e.into_string()))?;

        Ok(Self {
            log,
            db,
            write_opts,
        })
    }

    pub(crate) fn index(
        &self,
        stream_id: i64,
        offset: u64,
        wal_offset: u64,
        len: u32,
        hash: u64,
    ) -> Result<(), StoreError> {
        match self.db.cf_handle(INDEX_COLUMN_FAMILY) {
            Some(cf) => {
                let mut key_buf = BytesMut::with_capacity(16);
                key_buf.put_i64(stream_id);
                key_buf.put_u64(offset);

                let mut value_buf = BytesMut::with_capacity(20);
                value_buf.put_u64(wal_offset);
                value_buf.put_u32(len);
                value_buf.put_u64(hash);
                self.db
                    .put_cf_opt(cf, &key_buf[..], &value_buf[..], &self.write_opts)
                    .map_err(|e| StoreError::RocksDB(e.into_string()))
            }
            None => Err(StoreError::RocksDB("No column family".to_owned())),
        }
    }

    /// Returns WAL checkpoint offset.
    ///
    /// Note we can call this method as frequently as we need as all operation is memory
    pub(crate) fn get_wal_checkpoint(&self) -> Result<u64, StoreError> {
        if let Some(metadata_cf) = self.db.cf_handle(METADATA_COLUMN_FAMILY) {
            match self
                .db
                .get_cf(metadata_cf, WAL_CHECKPOINT)
                .map_err(|e| StoreError::RocksDB(e.into_string()))?
            {
                Some(value) => {
                    if value.len() < 8 {
                        error!(self.log, "Value of wal_checkpoint is corrupted. Expecting 8 bytes in big endian, actual: {:?}", value);
                        return Err(StoreError::DataCorrupted);
                    }
                    let mut cursor = Cursor::new(&value[..]);
                    Ok(cursor.get_u64())
                }
                None => {
                    info!(
                        self.log,
                        "No KV entry for wal_checkpoint yet. Default wal_checkpoint to 0"
                    );
                    Ok(0)
                }
            }
        } else {
            info!(
                self.log,
                "No column family metadata yet. Default wal_checkpoint to 0"
            );
            Ok(0)
        }
    }

    pub(crate) fn advance_wal_checkpoint(&mut self, offset: u64) -> Result<(), StoreError> {
        let cf = self
            .db
            .cf_handle(METADATA_COLUMN_FAMILY)
            .ok_or(StoreError::RocksDB(
                "Metadata should have been created as we have create_if_missing enabled".to_owned(),
            ))?;
        let mut buf = BytesMut::with_capacity(8);
        buf.put_u64(offset);
        self.db
            .put_cf_opt(cf, WAL_CHECKPOINT, &buf[..], &self.write_opts)
            .map_err(|e| StoreError::RocksDB(e.into_string()))
    }

    /// Flush record index in cache into RocksDB using atomic-flush.
    ///
    /// Normally, we do not have invoke this function as frequently as insertion of entry, mapping stream offset to WAL
    /// offset. Reason behind this that DB is having `AtomicFlush` enabled. As long as we put mapping entries first and
    /// then update checkpoint `offset` of WAL.
    ///
    /// Once a memtable is full and flushed to SST files, we can guarantee that all mapping entries prior to checkpoint
    /// `offset` of WAL are already persisted.
    ///
    /// To minimize the amount of WAL data to recover, for example, in case of planned reboot, we shall update checkpoint
    /// offset and invoke this method before stopping.
    pub(crate) fn flush(&self) -> Result<(), StoreError> {
        self.db
            .flush()
            .map_err(|e| StoreError::RocksDB(e.into_string()))
    }

    /// Compaction is synchronous and should execute in its own thread.
    pub(crate) fn compact(&self) {
        if let Some(cf) = self.db.cf_handle(INDEX_COLUMN_FAMILY) {
            self.db.compact_range_cf(cf, None::<&[u8]>, None::<&[u8]>);
        }
    }
}

impl super::LocalRangeManager for Indexer {
    fn list_by_stream(&self, stream_id: i64) -> Result<Option<Vec<StreamRange>>, StoreError> {
        let mut prefix = BytesMut::with_capacity(9);
        prefix.put_u8(RANGE_PREFIX);
        prefix.put_i64(stream_id);

        if let Some(cf) = self.db.cf_handle(METADATA_COLUMN_FAMILY) {
            let mut read_opts = ReadOptions::default();
            read_opts.set_iterate_lower_bound(&prefix[..]);
            read_opts.set_prefix_same_as_start(true);
            let list = self
                .db
                .iterator_cf_opt(cf, read_opts, rocksdb::IteratorMode::Start)
                .flatten()
                .map_while(|(k, v)| {
                    if !k.starts_with(&prefix[..]) {
                        return None;
                    } else {
                        debug_assert_eq!(k.len(), 8 + 8 + 1);

                        let mut key_reader = Cursor::new(&k[..]);
                        let _prefix = key_reader.get_u8();
                        debug_assert_eq!(_prefix, RANGE_PREFIX);

                        let _stream_id = key_reader.get_i64();
                        debug_assert_eq!(stream_id, _stream_id);

                        let start = key_reader.get_u64();

                        if v.len() == 1 {
                            Some(StreamRange::new(start, 0, None))
                        } else {
                            debug_assert_eq!(v.len(), 8 + 1);
                            let mut value_reader = Cursor::new(&v[..]);
                            let _status = value_reader.get_u8();
                            let end = value_reader.get_u64();
                            Some(StreamRange::new(start, 0, Some(end)))
                        }
                    }
                })
                .collect();
            Ok(Some(list))
        } else {
            Ok(None)
        }
    }

    fn list(&self) -> Result<Option<Vec<StreamRange>>, StoreError> {
        let mut prefix = BytesMut::with_capacity(1);
        prefix.put_u8(RANGE_PREFIX);

        if let Some(cf) = self.db.cf_handle(METADATA_COLUMN_FAMILY) {
            let mut read_opts = ReadOptions::default();
            read_opts.set_iterate_lower_bound(&prefix[..]);
            read_opts.set_prefix_same_as_start(true);
            let list = self
                .db
                .iterator_cf_opt(cf, read_opts, rocksdb::IteratorMode::Start)
                .flatten()
                .map_while(|(k, v)| {
                    if !k.starts_with(&prefix[..]) {
                        return None;
                    } else {
                        debug_assert_eq!(k.len(), 8 + 8 + 1);

                        let mut key_reader = Cursor::new(&k[..]);
                        let _prefix = key_reader.get_u8();
                        debug_assert_eq!(_prefix, RANGE_PREFIX);

                        let _stream_id = key_reader.get_i64();

                        let start = key_reader.get_u64();

                        if v.len() == 1 {
                            debug_assert_eq!(0, v[0]);
                            Some(StreamRange::new(start, 0, None))
                        } else {
                            debug_assert_eq!(v.len(), 8 + 1);
                            let mut value_reader = Cursor::new(&v[..]);
                            let _status = value_reader.get_u8();
                            debug_assert_eq!(1u8, _status);
                            let end = value_reader.get_u64();
                            Some(StreamRange::new(start, 0, Some(end)))
                        }
                    }
                })
                .collect();
            Ok(Some(list))
        } else {
            Ok(None)
        }
    }

    fn seal(&self, stream_id: i64, range: &StreamRange) -> Result<(), StoreError> {
        debug_assert!(range.sealed(), "Range is not sealed yet");
        let end = range.end().ok_or(StoreError::Internal("".to_owned()))?;
        debug_assert!(end >= range.start(), "End of range cannot less than start");

        let mut key_buf = BytesMut::with_capacity(1 + 8 + 8);
        key_buf.put_u8(RANGE_PREFIX);
        key_buf.put_i64(stream_id);
        key_buf.put_u64(range.start());

        let mut value_buf = BytesMut::with_capacity(1 + 8);
        value_buf.put_u8(1);
        value_buf.put_u64(end);

        if let Some(cf) = self.db.cf_handle(METADATA_COLUMN_FAMILY) {
            self.db
                .put_cf_opt(cf, &key_buf[..], &value_buf[..], &self.write_opts)
                .map_err(|e| StoreError::RocksDB(e.into_string()))
        } else {
            Err(StoreError::RocksDB(format!(
                "No column family: {}",
                METADATA_COLUMN_FAMILY
            )))
        }
    }

    fn add(&self, stream_id: i64, range: &StreamRange) -> Result<(), StoreError> {
        let mut key_buf = BytesMut::with_capacity(1 + 8 + 8);
        key_buf.put_u8(RANGE_PREFIX);
        key_buf.put_i64(stream_id);
        key_buf.put_u64(range.start());

        let mut value_buf = BytesMut::with_capacity(1 + 8);
        if let Some(end) = range.end() {
            value_buf.put_u8(1);
            value_buf.put_u64(end);
        } else {
            value_buf.put_u8(0);
        }

        if let Some(cf) = self.db.cf_handle(METADATA_COLUMN_FAMILY) {
            self.db
                .put_cf_opt(cf, &key_buf[..], &value_buf[..], &self.write_opts)
                .map_err(|e| StoreError::RocksDB(e.into_string()))
        } else {
            Err(StoreError::RocksDB(format!(
                "No column family: {}",
                METADATA_COLUMN_FAMILY
            )))
        }
    }
}
