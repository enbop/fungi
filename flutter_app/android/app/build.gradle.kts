import java.util.Properties
import java.io.FileInputStream
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.security.MessageDigest

plugins {
    id("com.android.application")
    id("kotlin-android")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

val keystoreProperties = Properties()
val keystorePropertiesFile = rootProject.file("key.properties")
if (keystorePropertiesFile.exists()) {
    keystoreProperties.load(FileInputStream(keystorePropertiesFile))
}

tasks.register("copyRustBinary") {
    val projectRoot = project.rootProject.projectDir.parentFile.parentFile
    val sourceFile = File(projectRoot, "target/aarch64-linux-android/release/fungi")
    val targetDir = File(projectDir, "src/main/jniLibs/arm64-v8a")
    val targetFile = File(targetDir, "libfungi.so")
    
    doFirst {
        if (!sourceFile.exists()) {
            throw GradleException(
                "Rust binary not found at: ${sourceFile.absolutePath}\n" +
                "Please build the Rust project first using:\n" +
                "cargo ndk -P 24 -t arm64-v8a build --bin fungi --release"
            )
        }
        
        targetDir.mkdirs()
        
        val needsCopy = if (!targetFile.exists()) {
            println("Target file doesn't exist, copying...")
            true
        } else {
            val sourceHash = sourceFile.inputStream().use { input ->
                MessageDigest.getInstance("MD5").digest(input.readBytes()).joinToString("") { 
                    "%02x".format(it) 
                }
            }
            val targetHash = targetFile.inputStream().use { input ->
                MessageDigest.getInstance("MD5").digest(input.readBytes()).joinToString("") { 
                    "%02x".format(it) 
                }
            }
            
            if (sourceHash != targetHash) {
                println("File changed (MD5 mismatch), copying...")
                true
            } else {
                println("File unchanged, skipping copy")
                false
            }
        }
        
        if (needsCopy) {
            Files.copy(
                sourceFile.toPath(),
                targetFile.toPath(),
                StandardCopyOption.REPLACE_EXISTING
            )
            println("Copied Rust binary: ${sourceFile.absolutePath} -> ${targetFile.absolutePath}")
        }
    }
}

tasks.whenTaskAdded {
    if (name == "mergeDebugNativeLibs" || name == "mergeReleaseNativeLibs") {
        dependsOn("copyRustBinary")
    }
}

android {
    namespace = "rs.fungi.fungi_app"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = "29.0.13846066"

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }

    kotlinOptions {
        jvmTarget = JavaVersion.VERSION_11.toString()
    }

    defaultConfig {
        // TODO: Specify your own unique Application ID (https://developer.android.com/studio/build/application-id.html).
        applicationId = "rs.fungi.fungi_app"
        // You can update the following values to match your application needs.
        // For more information, see: https://flutter.dev/to/review-gradle-config.
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        versionCode = flutter.versionCode
        versionName = flutter.versionName
    }

    signingConfigs {
        create("release") {
            keyAlias = keystoreProperties["keyAlias"] as String
            keyPassword = keystoreProperties["keyPassword"] as String
            storeFile = keystoreProperties["storeFile"]?.let { file(it) }
            storePassword = keystoreProperties["storePassword"] as String
        }
    }

    buildTypes {
        release {
            signingConfig = signingConfigs.getByName("release")
        }
    }
}

flutter {
    source = "../.."
}
