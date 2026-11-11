//! What the Android app calls: `io.github.vaspyyy.lumen.core.Core`.
//!
//! Links and tabs live here and are known to the app by a number. Calls
//! that wait for the PC block the thread they come on: the app makes them
//! off its main thread. What a terminal has to say goes back through the
//! app's `TabEvents`, on threads of ours.

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicI32, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use ::jni::objects::{GlobalRef, JByteArray, JByteBuffer, JObject, JString, JValue};
use ::jni::sys::{jboolean, jint, jlong, jstring};
use ::jni::{JNIEnv, JavaVM};
use serde_json::json;
use tokio::sync::oneshot;

use crate::link::{self, Failure, Host, Link, Sign, Tab, TabEvents};
use crate::runtime;
use crate::screen::Size;

static LINKS: Mutex<Option<HashMap<i64, Arc<Link>>>> = Mutex::new(None);
static TABS: Mutex<Option<HashMap<i64, Arc<Tab>>>> = Mutex::new(None);
static NEXT: AtomicI64 = AtomicI64::new(1);
/// How far the pairing that is under way has come: 0 none, 1 reaching the
/// PC, 2 the PC's user is being asked.
static ENROLL_STAGE: AtomicI32 = AtomicI32::new(0);
/// What ends the pairing that is under way, if the app gives up on it.
static ENROLL_STOP: Mutex<Option<oneshot::Sender<()>>> = Mutex::new(None);

fn held<T>(all: &'static Mutex<Option<HashMap<i64, T>>>) -> MutexGuard<'static, Option<HashMap<i64, T>>> {
    let mut all = all.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    all.get_or_insert_with(HashMap::new);
    all
}

fn keep<T>(all: &'static Mutex<Option<HashMap<i64, T>>>, one: T) -> i64 {
    let number = NEXT.fetch_add(1, Ordering::Relaxed);
    held(all).as_mut().expect("made above").insert(number, one);
    number
}

fn find<T: Clone>(all: &'static Mutex<Option<HashMap<i64, T>>>, number: i64) -> Option<T> {
    held(all).as_ref().and_then(|all| all.get(&number).cloned())
}

fn forget<T>(all: &'static Mutex<Option<HashMap<i64, T>>>, number: i64) -> Option<T> {
    held(all).as_mut().and_then(|all| all.remove(&number))
}

/// Waits for work on the connections' threads to be done. The app may call
/// from one of those threads (out of a `TabEvents` method): then that thread
/// is handed back to the others while it waits.
fn wait<T>(work: impl Future<Output = T>) -> T {
    match tokio::runtime::Handle::try_current() {
        Ok(_) => tokio::task::block_in_place(|| runtime().handle().block_on(work)),
        Err(_) => runtime().block_on(work),
    }
}

/// A panic must not cross into the app: it would take the whole app down.
fn guarded<T>(otherwise: T, work: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(work)).unwrap_or(otherwise)
}

fn text(env: &mut JNIEnv, string: &JString) -> String {
    env.get_string(string).map(Into::into).unwrap_or_default()
}

fn give(env: &mut JNIEnv, string: &str) -> jstring {
    env.new_string(string).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
}

fn failure(failure: Failure) -> String {
    json!({ "ok": false, "code": failure.code, "error": failure.message }).to_string()
}

/// An object of the app that code of ours calls back, from any thread.
struct Callback {
    vm: JavaVM,
    target: GlobalRef,
}

impl Callback {
    fn new(env: &mut JNIEnv, target: &JObject) -> Option<Callback> {
        Some(Callback { vm: env.get_java_vm().ok()?, target: env.new_global_ref(target).ok()? })
    }

    fn call<T>(&self, work: impl FnOnce(&mut JNIEnv, &JObject) -> ::jni::errors::Result<T>) -> Option<T> {
        let mut env = self.vm.attach_current_thread_permanently().ok()?;
        let done = env.with_local_frame(8, |env| work(env, self.target.as_obj()));
        // what the app's code threw is the app's to see in its log, not ours to pass on
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_describe();
            let _ = env.exception_clear();
        }
        done.ok()
    }

    fn say(&self, method: &str, word: &str) {
        self.call(|env, target| {
            let word = env.new_string(word)?;
            env.call_method(target, method, "(Ljava/lang/String;)V", &[JValue::Object(&word)]).map(drop)
        });
    }
}

impl TabEvents for Callback {
    fn dirty(&self) {
        self.call(|env, target| env.call_method(target, "dirty", "()V", &[]).map(drop));
    }

    fn title(&self, title: &str) {
        self.say("title", title);
    }

    fn bell(&self) {
        self.call(|env, target| env.call_method(target, "bell", "()V", &[]).map(drop));
    }

    fn clipboard(&self, text: &str) {
        self.say("clipboard", text);
    }

    fn ended(&self, status: i32) {
        self.call(|env, target| env.call_method(target, "ended", "(I)V", &[JValue::Int(status)]).map(drop));
    }

    fn lost(&self, reason: &str) {
        self.say("lost", reason);
    }
}

impl Sign for Callback {
    fn sign(&self, data: &[u8]) -> Option<Vec<u8>> {
        self.call(|env, target| {
            let data = env.byte_array_from_slice(data)?;
            let signed = env.call_method(target, "sign", "([B)[B", &[JValue::Object(&data)])?.l()?;
            if signed.is_null() { Ok(None) } else { env.convert_byte_array(JByteArray::from(signed)).map(Some) }
        })
        .flatten()
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_version<'l>(
    mut env: JNIEnv<'l>,
    _core: JObject<'l>,
) -> jstring {
    give(&mut env, env!("CARGO_PKG_VERSION"))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_publicKey<'l>(
    mut env: JNIEnv<'l>,
    _core: JObject<'l>,
    spki: JByteArray<'l>,
) -> jstring {
    let line = guarded(None, || link::public_key_line(&env.convert_byte_array(&spki).ok()?));
    give(&mut env, &line.unwrap_or_default())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_fingerprint<'l>(
    mut env: JNIEnv<'l>,
    _core: JObject<'l>,
    public_key: JString<'l>,
) -> jstring {
    let line = text(&mut env, &public_key);
    let print = guarded(None, || lumen_link::fingerprint(&line).map(|print| lumen_link::fingerprint_text(&print)));
    give(&mut env, &print.unwrap_or_default())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_offer<'l>(
    mut env: JNIEnv<'l>,
    _core: JObject<'l>,
    link: JString<'l>,
) -> jstring {
    let link = text(&mut env, &link);
    let answer = guarded(String::new(), || match lumen_link::Offer::from_link(&link) {
        Ok(offer) => json!({
            "ok": true, "label": offer.label, "user": offer.user, "port": offer.port, "addresses": offer.addresses,
        })
        .to_string(),
        Err(e) => json!({ "ok": false, "error": e.to_string() }).to_string(),
    });
    give(&mut env, &answer)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_enroll<'l>(
    mut env: JNIEnv<'l>,
    _core: JObject<'l>,
    link: JString<'l>,
    device: JString<'l>,
    public_key: JString<'l>,
) -> jstring {
    let (link, device, public_key) = (text(&mut env, &link), text(&mut env, &device), text(&mut env, &public_key));
    let answer = guarded(String::new(), || {
        let (stop, stopped) = oneshot::channel();
        *ENROLL_STOP.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(stop);
        ENROLL_STAGE.store(1, Ordering::Relaxed);
        let done = wait(async {
            tokio::select! {
                done = link::enroll(&link, &device, &public_key, || ENROLL_STAGE.store(2, Ordering::Relaxed)) => done,
                // dropping the pairing drops its connection: the PC stops asking
                Ok(()) = stopped => Err(Failure { code: "cancelled", message: "Pairing was stopped.".into() }),
            }
        });
        ENROLL_STAGE.store(0, Ordering::Relaxed);
        match done {
            Ok(host) => json!({ "ok": true, "host": host }).to_string(),
            Err(e) => failure(e),
        }
    });
    give(&mut env, &answer)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_enrollStage<'l>(
    _env: JNIEnv<'l>,
    _core: JObject<'l>,
) -> jint {
    ENROLL_STAGE.load(Ordering::Relaxed)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_cancelEnroll<'l>(_env: JNIEnv<'l>, _core: JObject<'l>) {
    if let Some(stop) = ENROLL_STOP.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take() {
        let _ = stop.send(());
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_connect<'l>(
    mut env: JNIEnv<'l>,
    _core: JObject<'l>,
    host: JString<'l>,
    signer: JObject<'l>,
) -> jstring {
    let host = text(&mut env, &host);
    let signer = Callback::new(&mut env, &signer);
    let answer = guarded(String::new(), || {
        let Ok(host) = serde_json::from_str::<Host>(&host) else {
            return failure(Failure { code: "failed", message: "This phone's record of the PC is damaged.".into() });
        };
        let Some(signer) = signer else {
            return failure(Failure { code: "failed", message: "This phone's key can't be used.".into() });
        };
        match wait(link::connect(&host, Arc::new(signer))) {
            Ok(link) => {
                let (keeps, address) = (link.keeps(), link.address.clone());
                json!({ "ok": true, "link": keep(&LINKS, link), "keeps": keeps, "address": address }).to_string()
            },
            Err(e) => failure(e),
        }
    });
    give(&mut env, &answer)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_disconnect<'l>(
    _env: JNIEnv<'l>,
    _core: JObject<'l>,
    link: jlong,
) {
    guarded((), || {
        if let Some(link) = forget(&LINKS, link) {
            runtime().spawn(async move { link.disconnect().await });
        }
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_alive<'l>(
    _env: JNIEnv<'l>,
    _core: JObject<'l>,
    link: jlong,
    timeout_ms: jint,
) -> jboolean {
    guarded(false, || {
        let Some(link) = find(&LINKS, link) else { return false };
        wait(link.alive(Duration::from_millis(timeout_ms.clamp(100, 60_000) as u64)))
    }) as jboolean
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_sessions<'l>(
    mut env: JNIEnv<'l>,
    _core: JObject<'l>,
    link: jlong,
) -> jstring {
    let answer = guarded(None, || {
        let link = find(&LINKS, link)?;
        serde_json::to_string(&wait(link.sessions())).ok()
    });
    give(&mut env, &answer.unwrap_or_else(|| "[]".into()))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_kill<'l>(
    mut env: JNIEnv<'l>,
    _core: JObject<'l>,
    link: jlong,
    name: JString<'l>,
) {
    let name = text(&mut env, &name);
    guarded((), || {
        if let Some(link) = find(&LINKS, link) {
            wait(link.kill(&name));
        }
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_open<'l>(
    mut env: JNIEnv<'l>,
    _core: JObject<'l>,
    link: jlong,
    name: JString<'l>,
    cols: jint,
    rows: jint,
    cell_width: jint,
    cell_height: jint,
    events: JObject<'l>,
) -> jlong {
    let name = text(&mut env, &name);
    let events = Callback::new(&mut env, &events);
    guarded(0, || {
        let (Some(link), Some(events)) = (find(&LINKS, link), events) else { return 0 };
        let size = Size::new(cols, rows, cell_width, cell_height);
        keep(&TABS, link::open(link, &name, size, Arc::new(events)))
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_demo<'l>(
    mut env: JNIEnv<'l>,
    _core: JObject<'l>,
    cols: jint,
    rows: jint,
    events: JObject<'l>,
) -> jlong {
    let events = Callback::new(&mut env, &events);
    guarded(0, || {
        let Some(events) = events else { return 0 };
        keep(&TABS, link::demo(Size::new(cols, rows, 10, 20), Arc::new(events)))
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_close<'l>(
    _env: JNIEnv<'l>,
    _core: JObject<'l>,
    tab: jlong,
) {
    guarded((), || {
        if let Some(tab) = forget(&TABS, tab) {
            tab.close();
        }
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_resize<'l>(
    _env: JNIEnv<'l>,
    _core: JObject<'l>,
    tab: jlong,
    cols: jint,
    rows: jint,
    cell_width: jint,
    cell_height: jint,
) {
    guarded((), || {
        if let Some(tab) = find(&TABS, tab) {
            tab.resize(Size::new(cols, rows, cell_width, cell_height));
        }
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_text<'l>(
    mut env: JNIEnv<'l>,
    _core: JObject<'l>,
    tab: jlong,
    typed: JString<'l>,
    mods: jint,
) {
    let typed = text(&mut env, &typed);
    guarded((), || {
        if let Some(tab) = find(&TABS, tab) {
            tab.screen.scroll_to_bottom();
            tab.send(tab.screen.text(&typed, mods as u8));
            tab.changed();
        }
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_paste<'l>(
    mut env: JNIEnv<'l>,
    _core: JObject<'l>,
    tab: jlong,
    pasted: JString<'l>,
) {
    let pasted = text(&mut env, &pasted);
    guarded((), || {
        if let Some(tab) = find(&TABS, tab) {
            tab.screen.scroll_to_bottom();
            tab.send(tab.screen.paste(&pasted));
            tab.changed();
        }
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_key<'l>(
    _env: JNIEnv<'l>,
    _core: JObject<'l>,
    tab: jlong,
    key: jint,
    mods: jint,
) {
    guarded((), || {
        if let Some(tab) = find(&TABS, tab) {
            tab.screen.scroll_to_bottom();
            tab.send(tab.screen.key(key as u8, mods as u8));
            tab.changed();
        }
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_scroll<'l>(
    _env: JNIEnv<'l>,
    _core: JObject<'l>,
    tab: jlong,
    lines: jint,
    col: jint,
    row: jint,
) {
    guarded((), || {
        if let Some(tab) = find(&TABS, tab) {
            tab.send(tab.screen.scroll(lines, col.max(0) as usize, row.max(0) as usize));
            tab.changed();
        }
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_scrollToBottom<'l>(
    _env: JNIEnv<'l>,
    _core: JObject<'l>,
    tab: jlong,
) {
    guarded((), || {
        if let Some(tab) = find(&TABS, tab) {
            tab.screen.scroll_to_bottom();
            tab.changed();
        }
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_mouse<'l>(
    _env: JNIEnv<'l>,
    _core: JObject<'l>,
    tab: jlong,
    kind: jint,
    button: jint,
    col: jint,
    row: jint,
    mods: jint,
) -> jboolean {
    guarded(false, || {
        let Some(tab) = find(&TABS, tab) else { return false };
        match tab.screen.mouse(kind as u8, button as u8, col.max(0) as usize, row.max(0) as usize, mods as u8) {
            Some(report) => {
                tab.send(report);
                true
            },
            None => false,
        }
    }) as jboolean
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_select<'l>(
    _env: JNIEnv<'l>,
    _core: JObject<'l>,
    tab: jlong,
    kind: jint,
    col: jint,
    row: jint,
) {
    guarded((), || {
        if let Some(tab) = find(&TABS, tab) {
            tab.screen.select(kind as u8, col.max(0) as usize, row.max(0) as usize);
            tab.changed();
        }
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_selection<'l>(
    mut env: JNIEnv<'l>,
    _core: JObject<'l>,
    tab: jlong,
) -> jstring {
    match guarded(None, || find(&TABS, tab)?.screen.selection()) {
        Some(selected) => give(&mut env, &selected),
        None => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_linkAt<'l>(
    mut env: JNIEnv<'l>,
    _core: JObject<'l>,
    tab: jlong,
    col: jint,
    row: jint,
) -> jstring {
    match guarded(None, || find(&TABS, tab)?.screen.link_at(col.max(0) as usize, row.max(0) as usize)) {
        Some(link) => give(&mut env, &link),
        None => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_focus<'l>(
    _env: JNIEnv<'l>,
    _core: JObject<'l>,
    tab: jlong,
    focused: jboolean,
) {
    guarded((), || {
        if let Some(tab) = find(&TABS, tab) {
            tab.send(tab.screen.focus(focused != 0));
        }
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_vaspyyy_lumen_core_Core_snapshot<'l>(
    env: JNIEnv<'l>,
    _core: JObject<'l>,
    tab: jlong,
    into: JByteBuffer<'l>,
) -> jint {
    guarded(0, || {
        let Some(tab) = find(&TABS, tab) else { return 0 };
        let (Ok(start), Ok(room)) = (env.get_direct_buffer_address(&into), env.get_direct_buffer_capacity(&into))
        else {
            return 0;
        };
        if start.is_null() {
            return 0;
        }
        // SAFETY: a direct buffer's memory is `room` bytes at `start` and stays put while the app
        // holds the buffer, which it does for the length of this call; the app draws from it on
        // the one thread that also makes this call.
        let out = unsafe { std::slice::from_raw_parts_mut(start, room) };
        match tab.screen.snapshot(out) {
            Ok(written) => written as jint,
            Err(needed) => -(needed as jint),
        }
    })
}
