#[cfg(not(target_os = "macos"))]
compile_error!("Claudio Notes is macOS-only");

#[cfg(target_os = "macos")]
mod app;
#[cfg(target_os = "macos")]
mod chrome;

fn main() {
    #[cfg(target_os = "macos")]
    app::run();
}
