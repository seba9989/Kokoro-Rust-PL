#!/usr/bin/env python3
"""Generuje testdata/tiny_kokoro.onnx: atrapa modelu Kokoro o tym samym interfejsie.
  wejścia:  input_ids int64 [1,N], style float [1,256], speed float [1]
  wyjścia:  waveform float [1, 100*(N)] — wszystkie próbki = sum(input_ids) + sum(style)*speed
Wyjście zależy od WSZYSTKICH wejść i ma długość zależną od N (jak prawdziwy model)."""
import onnx, sys
from onnx import helper as h, TensorProto as T, numpy_helper as nh
import numpy as np

IDS = "tokens" if len(sys.argv) > 2 and sys.argv[2] == "bad" else "input_ids"   # "bad": zły interfejs (test checkIO)
ids = h.make_tensor_value_info(IDS, T.INT64, [1, None])
style = h.make_tensor_value_info("style", T.FLOAT, [1, 256])
speed = h.make_tensor_value_info("speed", T.FLOAT, [1])
wave = h.make_tensor_value_info("waveform", T.FLOAT, [1, None])
dur = h.make_tensor_value_info("duration", T.FLOAT, [1, None])

c = lambda name, arr: nh.from_array(np.array(arr), name)
nodes = [
    h.make_node("Cast", [IDS], ["ids_f"], to=T.FLOAT),
    h.make_node("ReduceSum", ["ids_f"], ["ids_sum"], keepdims=0),
    h.make_node("ReduceSum", ["style"], ["style_sum"], keepdims=0),
    h.make_node("ReduceSum", ["speed"], ["speed_s"], keepdims=0),
    h.make_node("Mul", ["style_sum", "speed_s"], ["style_x_speed"]),
    h.make_node("Add", ["ids_sum", "style_x_speed"], ["val"]),
    h.make_node("Shape", [IDS], ["shp"]),
    h.make_node("Gather", ["shp", "idx1"], ["n"], axis=0),                # N (skalar int64)
    h.make_node("Mul", ["n", "hundred"], ["m"]),
    h.make_node("Reshape", ["m", "one_d"], ["m1"]),
    h.make_node("Concat", ["one1", "m1"], ["out_shape"], axis=0),
    h.make_node("Expand", ["val", "out_shape"], ["waveform"]),
    h.make_node("Cast", [IDS], ["duration"], to=T.FLOAT),
]
init = [c("idx1", np.array(1, dtype=np.int64)), c("hundred", np.array(100, dtype=np.int64)),
        c("one_d", np.array([1], dtype=np.int64)), c("one1", np.array([1], dtype=np.int64))]
g = h.make_graph(nodes, "tiny_kokoro", [ids, style, speed], [wave, dur], initializer=init)
m = h.make_model(g, opset_imports=[h.make_opsetid("", 17)])
m.ir_version = 9
onnx.checker.check_model(m)
onnx.save(m, sys.argv[1])
print("saved", sys.argv[1])
