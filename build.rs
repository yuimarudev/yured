fn main() {
  let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

  if target_os == "freebsd" {
    println!("cargo:rustc-link-lib=iio");
    println!("cargo:rustc-link-search=native=/usr/local/lib");
  }
}
