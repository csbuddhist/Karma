# ONNX Runtime loads its JNI classes by name.
-keep class ai.onnxruntime.** { *; }
# Components the system instantiates by name are kept by AAPT rules; nothing else to add.
