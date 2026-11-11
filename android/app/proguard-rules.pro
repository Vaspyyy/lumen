# The native library (liblumen_mobile.so) finds these by name through JNI:
# the Core object and its native methods, and the Signer and TabEvents
# callbacks it calls back into. Keep the whole package as it is.
-keep class io.github.vaspyyy.lumen.core.** { *; }
-keep interface io.github.vaspyyy.lumen.core.** { *; }
-keepclasseswithmembernames,includedescriptorclasses class * {
    native <methods>;
}
