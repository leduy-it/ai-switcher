fn main() {
    #[cfg(unix)]
    if std::env::args().nth(1).as_deref() == Some("--desktop-runtime-proxy") {
        std::process::exit(ai_account_switcher_lib::desktop_runtime_proxy());
    }
    ai_account_switcher_lib::run();
}
