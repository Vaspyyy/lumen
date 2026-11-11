//! lumen on a phone: the terminal and the connection to the PC.
//!
//! The Android app (`android/`) draws and listens to the fingers; this is
//! everything behind that. [`screen`] is one terminal's grid, [`link`] the
//! SSH connection to a paired PC, [`jni`] what the app calls.

pub mod jni;
pub mod link;
pub mod palette;
pub mod screen;

use std::sync::OnceLock;

use tokio::runtime::{Builder, Runtime};

/// The threads the connections run on.
pub fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("lumen")
            .enable_all()
            .build()
            .expect("the system gives lumen no threads")
    })
}
