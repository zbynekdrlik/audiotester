fn main() {
    let now = chrono::Utc::now().format("%Y-%m-%d").to_string();
    println!("cargo:rustc-env=BUILD_DATE={}", now);

    // cpal 0.17's asio-sys uses Windows Registry APIs for ASIO driver enumeration
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rustc-link-lib=advapi32");
        println!("cargo:rustc-link-lib=ole32");
    }
}
