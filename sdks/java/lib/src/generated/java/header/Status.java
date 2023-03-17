// automatically generated by the FlatBuffers compiler, do not modify

package header;

import com.google.flatbuffers.BaseVector;
import com.google.flatbuffers.BooleanVector;
import com.google.flatbuffers.ByteVector;
import com.google.flatbuffers.Constants;
import com.google.flatbuffers.DoubleVector;
import com.google.flatbuffers.FlatBufferBuilder;
import com.google.flatbuffers.FloatVector;
import com.google.flatbuffers.IntVector;
import com.google.flatbuffers.LongVector;
import com.google.flatbuffers.ShortVector;
import com.google.flatbuffers.StringVector;
import com.google.flatbuffers.Struct;
import com.google.flatbuffers.Table;
import com.google.flatbuffers.UnionVector;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;

@SuppressWarnings("unused")
public final class Status extends Table {
  public static void ValidateVersion() { Constants.FLATBUFFERS_23_1_21(); }
  public static Status getRootAsStatus(ByteBuffer _bb) { return getRootAsStatus(_bb, new Status()); }
  public static Status getRootAsStatus(ByteBuffer _bb, Status obj) { _bb.order(ByteOrder.LITTLE_ENDIAN); return (obj.__assign(_bb.getInt(_bb.position()) + _bb.position(), _bb)); }
  public void __init(int _i, ByteBuffer _bb) { __reset(_i, _bb); }
  public Status __assign(int _i, ByteBuffer _bb) { __init(_i, _bb); return this; }

  public short code() { int o = __offset(4); return o != 0 ? bb.getShort(o + bb_pos) : 0; }
  public String message() { int o = __offset(6); return o != 0 ? __string(o + bb_pos) : null; }
  public ByteBuffer messageAsByteBuffer() { return __vector_as_bytebuffer(6, 1); }
  public ByteBuffer messageInByteBuffer(ByteBuffer _bb) { return __vector_in_bytebuffer(_bb, 6, 1); }
  public int detail(int j) { int o = __offset(8); return o != 0 ? bb.get(__vector(o) + j * 1) & 0xFF : 0; }
  public int detailLength() { int o = __offset(8); return o != 0 ? __vector_len(o) : 0; }
  public ByteVector detailVector() { return detailVector(new ByteVector()); }
  public ByteVector detailVector(ByteVector obj) { int o = __offset(8); return o != 0 ? obj.__assign(__vector(o), bb) : null; }
  public ByteBuffer detailAsByteBuffer() { return __vector_as_bytebuffer(8, 1); }
  public ByteBuffer detailInByteBuffer(ByteBuffer _bb) { return __vector_in_bytebuffer(_bb, 8, 1); }

  public static int createStatus(FlatBufferBuilder builder,
      short code,
      int messageOffset,
      int detailOffset) {
    builder.startTable(3);
    Status.addDetail(builder, detailOffset);
    Status.addMessage(builder, messageOffset);
    Status.addCode(builder, code);
    return Status.endStatus(builder);
  }

  public static void startStatus(FlatBufferBuilder builder) { builder.startTable(3); }
  public static void addCode(FlatBufferBuilder builder, short code) { builder.addShort(0, code, 0); }
  public static void addMessage(FlatBufferBuilder builder, int messageOffset) { builder.addOffset(1, messageOffset, 0); }
  public static void addDetail(FlatBufferBuilder builder, int detailOffset) { builder.addOffset(2, detailOffset, 0); }
  public static int createDetailVector(FlatBufferBuilder builder, byte[] data) { return builder.createByteVector(data); }
  public static int createDetailVector(FlatBufferBuilder builder, ByteBuffer data) { return builder.createByteVector(data); }
  public static void startDetailVector(FlatBufferBuilder builder, int numElems) { builder.startVector(1, numElems, 1); }
  public static int endStatus(FlatBufferBuilder builder) {
    int o = builder.endTable();
    return o;
  }

  public static final class Vector extends BaseVector {
    public Vector __assign(int _vector, int _element_size, ByteBuffer _bb) { __reset(_vector, _element_size, _bb); return this; }

    public Status get(int j) { return get(new Status(), j); }
    public Status get(Status obj, int j) {  return obj.__assign(__indirect(__element(j), bb), bb); }
  }
  public StatusT unpack() {
    StatusT _o = new StatusT();
    unpackTo(_o);
    return _o;
  }
  public void unpackTo(StatusT _o) {
    short _oCode = code();
    _o.setCode(_oCode);
    String _oMessage = message();
    _o.setMessage(_oMessage);
    int[] _oDetail = new int[detailLength()];
    for (int _j = 0; _j < detailLength(); ++_j) {_oDetail[_j] = detail(_j);}
    _o.setDetail(_oDetail);
  }
  public static int pack(FlatBufferBuilder builder, StatusT _o) {
    if (_o == null) return 0;
    int _message = _o.getMessage() == null ? 0 : builder.createString(_o.getMessage());
    int _detail = 0;
    if (_o.getDetail() != null) {
      byte[] __detail = new byte[_o.getDetail().length];
      int _j = 0;
      for (int _e : _o.getDetail()) { __detail[_j] = (byte) _e; _j++;}
      _detail = createDetailVector(builder, __detail);
    }
    return createStatus(
      builder,
      _o.getCode(),
      _message,
      _detail);
  }
}

