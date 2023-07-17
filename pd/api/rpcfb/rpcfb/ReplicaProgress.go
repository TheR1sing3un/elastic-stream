// Code generated by the FlatBuffers compiler. DO NOT EDIT.

package rpcfb

import (
	flatbuffers "github.com/google/flatbuffers/go"
)

type ReplicaProgressT struct {
	StreamId int64 `json:"stream_id"`
	RangeIndex int32 `json:"range_index"`
	ConfirmOffset int64 `json:"confirm_offset"`
}

func (t *ReplicaProgressT) Pack(builder *flatbuffers.Builder) flatbuffers.UOffsetT {
	if t == nil { return 0 }
	ReplicaProgressStart(builder)
	ReplicaProgressAddStreamId(builder, t.StreamId)
	ReplicaProgressAddRangeIndex(builder, t.RangeIndex)
	ReplicaProgressAddConfirmOffset(builder, t.ConfirmOffset)
	return ReplicaProgressEnd(builder)
}

func (rcv *ReplicaProgress) UnPackTo(t *ReplicaProgressT) {
	t.StreamId = rcv.StreamId()
	t.RangeIndex = rcv.RangeIndex()
	t.ConfirmOffset = rcv.ConfirmOffset()
}

func (rcv *ReplicaProgress) UnPack() *ReplicaProgressT {
	if rcv == nil { return nil }
	t := &ReplicaProgressT{}
	rcv.UnPackTo(t)
	return t
}

type ReplicaProgress struct {
	_tab flatbuffers.Table
}

func GetRootAsReplicaProgress(buf []byte, offset flatbuffers.UOffsetT) *ReplicaProgress {
	n := flatbuffers.GetUOffsetT(buf[offset:])
	x := &ReplicaProgress{}
	x.Init(buf, n+offset)
	return x
}

func GetSizePrefixedRootAsReplicaProgress(buf []byte, offset flatbuffers.UOffsetT) *ReplicaProgress {
	n := flatbuffers.GetUOffsetT(buf[offset+flatbuffers.SizeUint32:])
	x := &ReplicaProgress{}
	x.Init(buf, n+offset+flatbuffers.SizeUint32)
	return x
}

func (rcv *ReplicaProgress) Init(buf []byte, i flatbuffers.UOffsetT) {
	rcv._tab.Bytes = buf
	rcv._tab.Pos = i
}

func (rcv *ReplicaProgress) Table() flatbuffers.Table {
	return rcv._tab
}

func (rcv *ReplicaProgress) StreamId() int64 {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(4))
	if o != 0 {
		return rcv._tab.GetInt64(o + rcv._tab.Pos)
	}
	return 0
}

func (rcv *ReplicaProgress) MutateStreamId(n int64) bool {
	return rcv._tab.MutateInt64Slot(4, n)
}

func (rcv *ReplicaProgress) RangeIndex() int32 {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(6))
	if o != 0 {
		return rcv._tab.GetInt32(o + rcv._tab.Pos)
	}
	return 0
}

func (rcv *ReplicaProgress) MutateRangeIndex(n int32) bool {
	return rcv._tab.MutateInt32Slot(6, n)
}

func (rcv *ReplicaProgress) ConfirmOffset() int64 {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(8))
	if o != 0 {
		return rcv._tab.GetInt64(o + rcv._tab.Pos)
	}
	return 0
}

func (rcv *ReplicaProgress) MutateConfirmOffset(n int64) bool {
	return rcv._tab.MutateInt64Slot(8, n)
}

func ReplicaProgressStart(builder *flatbuffers.Builder) {
	builder.StartObject(3)
}
func ReplicaProgressAddStreamId(builder *flatbuffers.Builder, streamId int64) {
	builder.PrependInt64Slot(0, streamId, 0)
}
func ReplicaProgressAddRangeIndex(builder *flatbuffers.Builder, rangeIndex int32) {
	builder.PrependInt32Slot(1, rangeIndex, 0)
}
func ReplicaProgressAddConfirmOffset(builder *flatbuffers.Builder, confirmOffset int64) {
	builder.PrependInt64Slot(2, confirmOffset, 0)
}
func ReplicaProgressEnd(builder *flatbuffers.Builder) flatbuffers.UOffsetT {
	return builder.EndObject()
}
