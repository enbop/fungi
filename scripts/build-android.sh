set -e

if [ "$1" == "debug" ]; then
    if [ "$2" == "x86_64" ]; then
        target_folder="x86_64-linux-android"
    elif [ "$2" == "arm64-v8a" ]; then
        target_folder="aarch64-linux-android"
    else
        echo "Unknown abi type: $2"
        exit 1
    fi

    cargo ndk \
        -t $2 \
        -o dist/android-output/jniLibs build \
        -p fungi-daemon-uniffi-binding

    cargo run -p uniffi-bindgen generate \
        --library target/$target_folder/debug/libfungi_daemon_binding.so \
        --language kotlin \
        --out-dir dist/android-binding/

    exit 0
fi

# default release build

cargo ndk \
    -t x86_64 \
    -t arm64-v8a \
    -o dist/android-output/jniLibs build \
    -p fungi-daemon-uniffi-binding --release

cargo run -p uniffi-bindgen generate \
    --library target/x86_64-linux-android/release/libfungi_daemon_binding.so \
    --language kotlin \
    --out-dir dist/android-binding/

tar -czvf dist/fungi-android.tar.gz dist/android-output dist/android-binding