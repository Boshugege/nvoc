fn main() {
    // A Python extension resolves Python symbols in the importing interpreter.
    // Cargo does not infer PyO3's cdylib linker flags from a dependency feature.
    // Apply them only to macOS extension builds; embedded tests still link Python.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos")
        && std::env::var_os("CARGO_FEATURE_EXTENSION_MODULE").is_some()
    {
        println!("cargo:rustc-cdylib-link-arg=-undefined");
        println!("cargo:rustc-cdylib-link-arg=dynamic_lookup");
    }
}
