# kotlinx.serialization: i modelli dell'API restano leggibili.
-keepattributes *Annotation*, InnerClasses
-dontnote kotlinx.serialization.**
-keepclassmembers class kotlinx.serialization.json.** { *** Companion; }
-keepclasseswithmembers class kotlinx.serialization.json.** { kotlinx.serialization.KSerializer serializer(...); }
-keep,includedescriptorclasses class app.platea.**$$serializer { *; }
-keepclassmembers class app.platea.** { *** Companion; }
-keepclasseswithmembers class app.platea.** { kotlinx.serialization.KSerializer serializer(...); }
-dontwarn org.conscrypt.**
-dontwarn org.bouncycastle.**
-dontwarn org.openjsse.**
# I corpi delle richieste si serializzano cercando il serializer dalla classe: classi e companion restano interi.
-keep @kotlinx.serialization.Serializable class app.platea.** { *; }
-keep class app.platea.**$Companion { *; }
