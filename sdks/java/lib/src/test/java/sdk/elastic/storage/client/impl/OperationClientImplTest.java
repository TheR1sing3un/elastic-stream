package sdk.elastic.storage.client.impl;

import com.google.flatbuffers.FlatBufferBuilder;
import java.nio.ByteBuffer;
import java.time.Duration;
import java.util.Collections;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutionException;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.mockito.Mock;
import sdk.elastic.storage.apis.OperationClient;
import sdk.elastic.storage.apis.manager.ResourceManager;
import sdk.elastic.storage.client.common.ClientId;
import sdk.elastic.storage.client.netty.NettyClient;
import sdk.elastic.storage.client.protocol.SbpFrame;
import sdk.elastic.storage.client.route.Address;
import sdk.elastic.storage.flatc.header.AppendResponseT;
import sdk.elastic.storage.flatc.header.AppendResultT;
import sdk.elastic.storage.flatc.header.ClientRole;
import sdk.elastic.storage.flatc.header.DataNodeT;
import sdk.elastic.storage.flatc.header.DescribeRangeResultT;
import sdk.elastic.storage.flatc.header.ErrorCode;
import sdk.elastic.storage.flatc.header.FetchResponseT;
import sdk.elastic.storage.flatc.header.FetchResultT;
import sdk.elastic.storage.flatc.header.HeartbeatResponse;
import sdk.elastic.storage.flatc.header.HeartbeatResponseT;
import sdk.elastic.storage.flatc.header.RangeT;
import sdk.elastic.storage.flatc.header.ReplicaNodeT;
import sdk.elastic.storage.flatc.header.StatusT;
import sdk.elastic.storage.models.OperationCode;
import sdk.elastic.storage.models.RecordBatch;
import sdk.elastic.storage.models.RecordBatchesGenerator;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.doReturn;
import static org.mockito.Mockito.mock;

class OperationClientImplTest {

    @Mock
    private NettyClient nettyClient;
    @Mock
    private ResourceManager resourceManager;
    @Mock
    private StreamRangeCache streamRangeCache;
    private OperationClient operationClient;
    private static final Address defaultPmAddress = new Address("localhost", 2080);
    private static final Duration DEFAULT_REQUEST_TIMEOUT = Duration.ofSeconds(3);
    private static StatusT okStatus;
    private static ReplicaNodeT replicaNodeT;

    @BeforeAll
    static void beforeAll() {
        okStatus = new StatusT();
        okStatus.setCode(ErrorCode.OK);
        okStatus.setMessage("OK");

        DataNodeT dataNodeT = new DataNodeT();
        dataNodeT.setNodeId(1);
        dataNodeT.setAdvertiseAddr("localhost:8080");

        replicaNodeT = new ReplicaNodeT();
        replicaNodeT.setDataNode(dataNodeT);
        replicaNodeT.setIsPrimary(true);
    }

    @BeforeEach
    void setUp() {
        nettyClient = mock(NettyClient.class);
        resourceManager = mock(ResourceManager.class);
        streamRangeCache = mock(StreamRangeCache.class);
        operationClient = new OperationClientImpl(Duration.ofSeconds(10), nettyClient, resourceManager, streamRangeCache);
    }

    @Test
    void appendBatch() throws ExecutionException, InterruptedException {
        long streamId = 1;
        long batchAppendBaseOffset = 110;
        long lastRangeBaseOffset = batchAppendBaseOffset - 10;
        long appendTimestamp = System.currentTimeMillis();

        RangeT rangeT = new RangeT();
        rangeT.setStreamId(streamId);
        rangeT.setReplicaNodes(new ReplicaNodeT[] {replicaNodeT});
        rangeT.setRangeIndex(1);
        rangeT.setStartOffset(lastRangeBaseOffset);
        rangeT.setEndOffset(-1);
        rangeT.setNextOffset(batchAppendBaseOffset);
        doReturn(CompletableFuture.completedFuture(rangeT)).when(streamRangeCache).getLastRange(streamId);

        AppendResultT appendResultT = new AppendResultT();
        appendResultT.setStreamId(streamId);
        appendResultT.setRequestIndex(0);
        appendResultT.setBaseOffset(batchAppendBaseOffset);
        appendResultT.setStreamAppendTimeMs(appendTimestamp);
        appendResultT.setStatus(okStatus);

        AppendResponseT appendResponseT = new AppendResponseT();
        appendResponseT.setStatus(okStatus);
        appendResponseT.setThrottleTimeMs(0);
        appendResponseT.setAppendResponses(new AppendResultT[] {appendResultT});
        FlatBufferBuilder builder = new FlatBufferBuilder();
        int appendResponseOffset = sdk.elastic.storage.flatc.header.AppendResponse.pack(builder, appendResponseT);
        builder.finish(appendResponseOffset);

        SbpFrame mockResponseFrame = SbpFrame.newBuilder()
            .setFlag((byte) (SbpFrame.GENERAL_RESPONSE_FLAG | SbpFrame.ENDING_RESPONSE_FLAG))
            .setOperationCode(OperationCode.APPEND.getCode())
            .setHeader(builder.dataBuffer())
            .build();
        doReturn(CompletableFuture.completedFuture(mockResponseFrame)).when(nettyClient).invokeAsync(any(Address.class), any(SbpFrame.class), any(Duration.class));

        RecordBatch batch = RecordBatchesGenerator.generateOneRecordBatch(streamId);
        AppendResultT resultT = operationClient.appendBatch(batch, DEFAULT_REQUEST_TIMEOUT).get();
        assertEquals(streamId, resultT.getStreamId());
        assertEquals(0, resultT.getRequestIndex());
        assertTrue(resultT.getBaseOffset() >= lastRangeBaseOffset);
        assertEquals(batchAppendBaseOffset, resultT.getBaseOffset());
        assertEquals(appendTimestamp, resultT.getStreamAppendTimeMs());
        assertEquals(ErrorCode.OK, resultT.getStatus().getCode());
    }

    @Test
    void getLastWritableOffset() throws ExecutionException, InterruptedException {
        long streamId = 2;
        long baseOffset = 110;
        long nextOffset = baseOffset + 15;
        long realNextOffset = nextOffset + 20;

        RangeT rangeT = new RangeT();
        rangeT.setStreamId(streamId);
        rangeT.setReplicaNodes(new ReplicaNodeT[] {replicaNodeT});
        rangeT.setRangeIndex(1);
        rangeT.setStartOffset(baseOffset);
        rangeT.setEndOffset(-1);
        rangeT.setNextOffset(nextOffset);
        doReturn(CompletableFuture.completedFuture(rangeT)).when(streamRangeCache).getLastRange(streamId);

        RangeT realRangeT = new RangeT();
        realRangeT.setStreamId(rangeT.getStreamId());
        realRangeT.setReplicaNodes(rangeT.getReplicaNodes());
        realRangeT.setRangeIndex(rangeT.getRangeIndex());
        realRangeT.setStartOffset(rangeT.getStartOffset());
        realRangeT.setEndOffset(rangeT.getEndOffset());
        realRangeT.setNextOffset(realNextOffset);
        DescribeRangeResultT describeRangeResultT = new DescribeRangeResultT();
        describeRangeResultT.setStatus(okStatus);
        describeRangeResultT.setStreamId(streamId);
        describeRangeResultT.setRange(realRangeT);
        doReturn(CompletableFuture.completedFuture(Collections.singletonList(describeRangeResultT))).when(resourceManager).describeRanges(any(Address.class), any(), any(Duration.class));

        Long aLong = operationClient.getLastWritableOffset(streamId, DEFAULT_REQUEST_TIMEOUT).get();
        assertTrue(aLong >= nextOffset);
    }

    @Test
    void fetchBatches() throws ExecutionException, InterruptedException {
        long streamId = 3;
        long startOffset = 89;
        long baseOffset = 100;
        long endOffset = baseOffset + 50;
        int minBytes = 10;
        int maxBytes = Integer.MAX_VALUE - 1;

        RecordBatch batch = RecordBatchesGenerator.generateOneRecordBatch(streamId, baseOffset);
        ByteBuffer encodedBuffer = batch.encode();

        RangeT rangeT = new RangeT();
        rangeT.setStreamId(streamId);
        rangeT.setReplicaNodes(new ReplicaNodeT[] {replicaNodeT});
        rangeT.setRangeIndex(1);
        rangeT.setStartOffset(baseOffset);
        rangeT.setEndOffset(endOffset);
        rangeT.setNextOffset(endOffset);
        doReturn(CompletableFuture.completedFuture(rangeT)).when(streamRangeCache).getFloorRange(streamId, startOffset);

        FetchResultT fetchResultT = new FetchResultT();
        fetchResultT.setStreamId(streamId);
        fetchResultT.setRequestIndex(0);
        fetchResultT.setBatchLength(encodedBuffer.remaining());
        fetchResultT.setStatus(okStatus);

        FetchResponseT fetchResponseT = new FetchResponseT();
        fetchResponseT.setStatus(okStatus);
        fetchResponseT.setThrottleTimeMs(0);
        fetchResponseT.setFetchResponses(new FetchResultT[] {fetchResultT});
        FlatBufferBuilder builder = new FlatBufferBuilder();
        int fetchResponseOffset = sdk.elastic.storage.flatc.header.FetchResponse.pack(builder, fetchResponseT);
        builder.finish(fetchResponseOffset);

        SbpFrame mockResponseFrame = SbpFrame.newBuilder()
            .setFlag((byte) (SbpFrame.GENERAL_RESPONSE_FLAG | SbpFrame.ENDING_RESPONSE_FLAG))
            .setOperationCode(OperationCode.FETCH.getCode())
            .setHeader(builder.dataBuffer())
            .setPayload(new ByteBuffer[] {encodedBuffer.duplicate()})
            .build();
        doReturn(CompletableFuture.completedFuture(mockResponseFrame)).when(nettyClient).invokeAsync(any(Address.class), any(SbpFrame.class), any(Duration.class));

        List<RecordBatch> batches = operationClient.fetchBatches(streamId, startOffset, minBytes, maxBytes, DEFAULT_REQUEST_TIMEOUT).get();
        Integer totalBytes = batches.stream().reduce(0, (sum, item) -> sum + item.encode().remaining(), Integer::sum);
        assertTrue(totalBytes >= minBytes);
        assertTrue(totalBytes <= maxBytes);
        assertEquals(1, batches.size());
        assertTrue(batches.get(0).getBaseOffset() >= startOffset);
        assertEquals(batch.getRecords(), batches.get(0).getRecords());
    }

    @Test
    void heartbeat() throws ExecutionException, InterruptedException {
        doReturn(new ClientId()).when(nettyClient).getClientId();

        ClientId clientId = operationClient.getClientId();
        byte clientRole = ClientRole.CLIENT_ROLE_CUSTOMER;
        HeartbeatResponseT heartbeatResponseT = new HeartbeatResponseT();
        heartbeatResponseT.setStatus(okStatus);
        heartbeatResponseT.setClientId(clientId.toString());
        heartbeatResponseT.setClientRole(clientRole);
        FlatBufferBuilder builder = new FlatBufferBuilder();
        int heartbeatResponseOffset = HeartbeatResponse.pack(builder, heartbeatResponseT);
        builder.finish(heartbeatResponseOffset);

        SbpFrame mockResponseFrame = SbpFrame.newBuilder()
            .setFlag((byte) (SbpFrame.GENERAL_RESPONSE_FLAG | SbpFrame.ENDING_RESPONSE_FLAG))
            .setOperationCode(OperationCode.HEARTBEAT.getCode())
            .setHeader(builder.dataBuffer())
            .build();

        doReturn(CompletableFuture.completedFuture(mockResponseFrame)).when(nettyClient).invokeAsync(any(Address.class), any(SbpFrame.class), any(Duration.class));
        Boolean result = operationClient.heartbeat(defaultPmAddress, DEFAULT_REQUEST_TIMEOUT).get();
        assertTrue(result);
    }

    @Test
    void getClientId() {
        doReturn(new ClientId()).when(nettyClient).getClientId();

        ClientId clientId = new ClientId();
        assertTrue(clientId.getIndex() > operationClient.getClientId().getIndex());
    }
}