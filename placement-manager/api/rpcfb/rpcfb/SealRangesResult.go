// Code generated by the FlatBuffers compiler. DO NOT EDIT.

package rpcfb

import (
	flatbuffers "github.com/google/flatbuffers/go"
)

type SealRangesResultT struct {
	Status *StatusT `json:"status"`
	Range *RangeT `json:"range"`
}

func (t *SealRangesResultT) Pack(builder *flatbuffers.Builder) flatbuffers.UOffsetT {
	if t == nil { return 0 }
	statusOffset := t.Status.Pack(builder)
	range_Offset := t.Range.Pack(builder)
	SealRangesResultStart(builder)
	SealRangesResultAddStatus(builder, statusOffset)
	SealRangesResultAddRange(builder, range_Offset)
	return SealRangesResultEnd(builder)
}

func (rcv *SealRangesResult) UnPackTo(t *SealRangesResultT) {
	t.Status = rcv.Status(nil).UnPack()
	t.Range = rcv.Range(nil).UnPack()
}

func (rcv *SealRangesResult) UnPack() *SealRangesResultT {
	if rcv == nil { return nil }
	t := &SealRangesResultT{}
	rcv.UnPackTo(t)
	return t
}

type SealRangesResult struct {
	_tab flatbuffers.Table
}

func GetRootAsSealRangesResult(buf []byte, offset flatbuffers.UOffsetT) *SealRangesResult {
	n := flatbuffers.GetUOffsetT(buf[offset:])
	x := &SealRangesResult{}
	x.Init(buf, n+offset)
	return x
}

func GetSizePrefixedRootAsSealRangesResult(buf []byte, offset flatbuffers.UOffsetT) *SealRangesResult {
	n := flatbuffers.GetUOffsetT(buf[offset+flatbuffers.SizeUint32:])
	x := &SealRangesResult{}
	x.Init(buf, n+offset+flatbuffers.SizeUint32)
	return x
}

func (rcv *SealRangesResult) Init(buf []byte, i flatbuffers.UOffsetT) {
	rcv._tab.Bytes = buf
	rcv._tab.Pos = i
}

func (rcv *SealRangesResult) Table() flatbuffers.Table {
	return rcv._tab
}

func (rcv *SealRangesResult) Status(obj *Status) *Status {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(4))
	if o != 0 {
		x := rcv._tab.Indirect(o + rcv._tab.Pos)
		if obj == nil {
			obj = new(Status)
		}
		obj.Init(rcv._tab.Bytes, x)
		return obj
	}
	return nil
}

func (rcv *SealRangesResult) Range(obj *Range) *Range {
	o := flatbuffers.UOffsetT(rcv._tab.Offset(6))
	if o != 0 {
		x := rcv._tab.Indirect(o + rcv._tab.Pos)
		if obj == nil {
			obj = new(Range)
		}
		obj.Init(rcv._tab.Bytes, x)
		return obj
	}
	return nil
}

func SealRangesResultStart(builder *flatbuffers.Builder) {
	builder.StartObject(2)
}
func SealRangesResultAddStatus(builder *flatbuffers.Builder, status flatbuffers.UOffsetT) {
	builder.PrependUOffsetTSlot(0, flatbuffers.UOffsetT(status), 0)
}
func SealRangesResultAddRange(builder *flatbuffers.Builder, range_ flatbuffers.UOffsetT) {
	builder.PrependUOffsetTSlot(1, flatbuffers.UOffsetT(range_), 0)
}
func SealRangesResultEnd(builder *flatbuffers.Builder) flatbuffers.UOffsetT {
	return builder.EndObject()
}
