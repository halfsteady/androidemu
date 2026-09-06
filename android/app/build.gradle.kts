plugins { id("com.android.application"); id("org.jetbrains.kotlin.android"); id("org.jetbrains.kotlin.plugin.compose") }
android {
    namespace = "dev.androidemu"
    compileSdk = 36
    defaultConfig {
        applicationId = "dev.androidemu"
        minSdk = 29
        targetSdk = 36
        versionCode = providers.gradleProperty("buildNumber").orElse("1").get().toInt()
        versionName = providers.gradleProperty("buildLabel").orElse("0.1.0-dev").get()
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }
    buildTypes {
        getByName("debug") { ndk { abiFilters += listOf("arm64-v8a", "x86_64") } }
        getByName("release") { ndk { abiFilters += "arm64-v8a" } }
        create("preview") {
            initWith(getByName("release"))
            signingConfig = signingConfigs.getByName("debug")
            matchingFallbacks += "release"
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
