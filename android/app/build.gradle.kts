import java.io.FileInputStream
import java.time.LocalDate
import java.time.ZoneOffset
import java.time.temporal.ChronoUnit
import java.util.Properties

plugins { id("com.android.application"); id("org.jetbrains.kotlin.android"); id("org.jetbrains.kotlin.plugin.compose") }

// Upload-key credentials for signed builds, from a gitignored
// android/keystore.properties or, failing that, ANDROIDEMU_UPLOAD_* environment
// variables so CI can supply them without a file in the repo.
val keystoreProps = Properties().apply {
    val file = rootProject.file("keystore.properties")
    if (file.exists()) FileInputStream(file).use { load(it) }
}
fun signingProp(key: String, env: String): String? = keystoreProps.getProperty(key) ?: System.getenv(env)

android {
    // The Kotlin package, and so the JNI symbol names in native/. It is
    // deliberately not the applicationId: renaming it would rename every
    // Java_dev_androidemu_Native_* export.
    namespace = "dev.androidemu"
    compileSdk = 36
    // Lets AGP find llvm-strip, so the Rust .so ships stripped.
    ndkVersion = "28.2.13676358"
    defaultConfig {
        applicationId = "com.bsteinfeld.amelianes"
        minSdk = 29
        targetSdk = 36
        // An unlabelled build still has to sort above whatever is already on the
        // tablet, or Android refuses the update. Days since the project epoch,
        // shifted four places, leaves room for the HHMM that a published build
        // passes in explicitly, and only changes once a day so incremental
        // builds stay up to date.
        val buildDate = LocalDate.now(ZoneOffset.UTC)
        val buildDay = ChronoUnit.DAYS.between(LocalDate.of(2018, 7, 9), buildDate)
        versionCode = providers.gradleProperty("buildNumber").orElse(
            System.getenv("ANDROIDEMU_VERSION_CODE") ?: (buildDay * 10000).toString()
        ).get().toInt()
        versionName = providers.gradleProperty("buildLabel").orElse(
            System.getenv("ANDROIDEMU_VERSION_NAME") ?: "0.1.0+$buildDate"
        ).get()
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }
    signingConfigs {
        // Only define a real signer when the keystore is actually present.
        // Pointing storeFile at a missing file fails configuration for every
        // task, debug included, so absent credentials fall back to debug
        // signing and a sideloadable - but not publishable - build.
        val store = signingProp("storeFile", "ANDROIDEMU_UPLOAD_STORE_FILE")?.let { rootProject.file(it) }
        if (store != null && store.exists()) {
            create("upload") {
                storeFile = store
                storePassword = signingProp("storePassword", "ANDROIDEMU_UPLOAD_STORE_PASSWORD")
                keyAlias = signingProp("keyAlias", "ANDROIDEMU_UPLOAD_KEY_ALIAS")
                keyPassword = signingProp("keyPassword", "ANDROIDEMU_UPLOAD_KEY_PASSWORD")
            }
        }
    }
    buildTypes {
        // A distinct id and the debug key, so a dev build never collides with
        // the signed build installed on the tablet. x86_64 rides along for
        // emulators, which only run the host CPU's ABI at a usable speed.
        getByName("debug") {
            applicationIdSuffix = ".debug"
            versionNameSuffix = "-debug"
            ndk { abiFilters += listOf("arm64-v8a", "x86_64") }
        }
        // The one shipping artifact: sideloaded from a GitHub Release today and
        // uploadable to Play as an AAB later, both signed with the upload key.
        getByName("release") {
            ndk { abiFilters += "arm64-v8a" }
            signingConfig = signingConfigs.findByName("upload") ?: signingConfigs.getByName("debug")
        }
    }
    buildFeatures { compose = true; buildConfig = true }
    compileOptions { sourceCompatibility = JavaVersion.VERSION_17; targetCompatibility = JavaVersion.VERSION_17 }
    kotlinOptions { jvmTarget = "17" }
}
dependencies {
    implementation(platform("androidx.compose:compose-bom:2025.06.01"))
    implementation("androidx.activity:activity-compose:1.10.1")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui-tooling-preview")
    androidTestImplementation("androidx.test:runner:1.6.2")
    androidTestImplementation("androidx.test.ext:junit:1.2.1")
    androidTestImplementation(platform("androidx.compose:compose-bom:2025.06.01"))
    androidTestImplementation("androidx.compose.ui:ui-test-junit4")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
    testImplementation("junit:junit:4.13.2")
}
val buildNative by tasks.registering(Exec::class) {
    workingDir = rootProject.projectDir.parentFile
    commandLine("bash", "scripts/build-native.sh")
    inputs.files(fileTree("../../core/src"), fileTree("../../native/src"), file("../../Cargo.toml"), file("../../native/Cargo.toml"), file("../../Cargo.lock"), file("../../scripts/build-native.sh"), file("../../.cargo/config.toml"))
    outputs.files("src/main/jniLibs/arm64-v8a/libnes_android.so", "src/main/jniLibs/x86_64/libnes_android.so")
}
tasks.named("preBuild").configure { dependsOn(buildNative) }

val buildHostNative by tasks.registering(Exec::class) {
    workingDir = rootProject.projectDir.parentFile
    commandLine("${System.getProperty("user.home")}/.cargo/bin/cargo", "build", "--locked", "--release", "-p", "nes-android")
    inputs.files(fileTree("../../core/src"), fileTree("../../native/src"), file("../../Cargo.lock"))
    outputs.file("../../target/release/libnes_android.so")
}
tasks.withType<Test>().configureEach {
    dependsOn(buildHostNative)
    inputs.file(rootProject.projectDir.parentFile.resolve("target/release/libnes_android.so"))
    systemProperty("java.library.path", rootProject.projectDir.parentFile.resolve("target/release").absolutePath)
}
