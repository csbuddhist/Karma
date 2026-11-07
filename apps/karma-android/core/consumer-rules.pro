# JNA reflects over its own classes and over the UniFFI-generated structures.
-dontwarn java.awt.**
-keep class com.sun.jna.** { *; }
-keepclassmembers class * extends com.sun.jna.** { public *; }
-keep class uniffi.karma_mobile_ffi.** { *; }
