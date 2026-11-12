// Modified by Longbridge for gpui-fast.
// Modified for lumen: `give_activation_token`.
#![cfg(any(target_os = "linux", target_os = "freebsd"))]
mod fast;
mod linux;

pub use linux::{current_platform, give_activation_token, linux_platform};
