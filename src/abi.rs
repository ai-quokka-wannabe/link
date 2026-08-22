//! The C boundary: the one module where `unsafe` is allowed, because the unsafety is its job.
//!
//! Everything here mirrors `include/lnk/lnk_client.h`, which is the surface a consumer that
//! `LoadLibrary()`s or `dlopen()`s Link actually sees: one exported symbol returning a vtable,
//! NULL when the asked-for version cannot be satisfied — the flagship's `tglGetProgramVTable`
//! refusal, reproduced because it has already proven itself. Every function behind the table
//! catches unwinding at the edge and answers with a status code: no panic crosses this
//! boundary, ever, which is the noexcept doctrine in library clothing. Null pointers are
//! refused with `LNK_BAD_ARGUMENT` rather than dereferenced with hope.

#![allow(unsafe_code)]

use std::ffi::{CStr, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;

use crate::codec::Message;
use crate::protocol::{
    Actions, CONTACTS_MAX, Contact, CreatureState, Derez, Event, Hello, MessageType, PROTOCOL_VERSION, Ping, Pong, Proprioception, REZ_MAX_MATERIALS, REZ_MAX_TRIANGLES,
    REZ_MAX_VERTICES, Rez, RezMaterial, RezTriangle, RezVertex, Role, TickStateHeader, Welcome, WorldDefinition, world_fingerprint,
};
use crate::recording::{Recorder, Replayer};
use crate::transport::{Connection, Listener, TransportError, connect, listen, local_hello, recorded_fingerprint};

/// What a client handle stands on: a socket, or a file in either direction.
enum End {
    Wire(Connection),
    Recording(Recorder),
    Replay(Replayer),
}

impl End {
    fn poll(&mut self) -> Result<Option<Message>, TransportError> {
        match self {
            End::Wire(connection) => connection.poll(),
            End::Recording(_) => Ok(None),
            End::Replay(replay) => replay.poll(),
        }
    }

    fn queue(&mut self, message: &Message) -> Result<(), crate::codec::EncodeError> {
        match self {
            End::Wire(connection) => connection.queue(message),
            End::Recording(recorder) => recorder.queue(message),
            // A replay has nobody to talk to; the refusal is the ABI's (LNK_BAD_ARGUMENT), and
            // the codec error here is only the shape the caller expects.
            End::Replay(_) => Err(crate::codec::EncodeError::ReservedNotZero),
        }
    }

    fn flush(&mut self) -> Result<bool, TransportError> {
        match self {
            End::Wire(connection) => connection.flush(),
            End::Recording(recorder) => recorder.flush(),
            End::Replay(_) => Ok(true),
        }
    }

    fn may_send_intents(&self) -> bool {
        match self {
            End::Wire(connection) => connection.may_send_intents(),
            End::Recording(_) => true,
            End::Replay(_) => false,
        }
    }

    fn may_send_proprioception(&self) -> bool {
        match self {
            End::Wire(connection) => connection.may_send_proprioception(),
            End::Recording(_) => true,
            End::Replay(_) => false,
        }
    }

    fn close(self) {
        match self {
            End::Wire(connection) => connection.close(),
            End::Recording(recorder) => recorder.close(),
            End::Replay(_) => {}
        }
    }
}

/// `LNK_CLIENT_ABI_VERSION`: bumped whenever the vtable or its rules change. The twin lives in
/// `lnk_client.h`, and a test holds the two together.
pub const LNK_CLIENT_ABI_VERSION: u32 = 6;

pub type LnkStatus = i32;

pub const LNK_OK: LnkStatus = 0;
pub const LNK_NOTHING_YET: LnkStatus = 1;
pub const LNK_REFUSED: LnkStatus = 2;
pub const LNK_HANDSHAKE_TIMED_OUT: LnkStatus = 3;
pub const LNK_PEER_CLOSED: LnkStatus = 4;
pub const LNK_FRAME_REFUSED: LnkStatus = 5;
pub const LNK_GARBLED: LnkStatus = 6;
pub const LNK_IO: LnkStatus = 7;
pub const LNK_BAD_ARGUMENT: LnkStatus = 8;
pub const LNK_PANIC: LnkStatus = 9;

/// The opaque handle the C side holds. Never constructed as itself: it is the public face of a
/// pointer to [`ClientInner`], and only the casts below relate the two.
#[repr(C)]
pub struct LnkClient {
    _opaque: [u8; 0],
}

struct ClientInner {
    end: End,
    /// The rows the last TICK_STATE view borrows from. Replaced on the next TICK_STATE, freed
    /// on close — exactly the lifetime the header promises the C side.
    tick_rows: Vec<CreatureState>,
    /// The last REZ's rows, under the same borrow rules as the tick's.
    rez_vertices: Vec<RezVertex>,
    rez_triangles: Vec<RezTriangle>,
    rez_materials: Vec<RezMaterial>,
    /// The last PROPRIOCEPTION's contacts, under the same borrow rules.
    contacts: Vec<Contact>,
}

/// The opaque handle a listening Master Control (or a test playing one) holds.
#[repr(C)]
pub struct LnkServer {
    _opaque: [u8; 0],
}

struct ServerInner {
    listener: Listener,
    /// The world this server hosts, as its fingerprint: every HELLO is judged against it.
    world_fingerprint: u64,
}

/// `LnkTickStateView`: the header by value, the rows by borrow from [`ClientInner::tick_rows`].
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TickStateView {
    pub header: TickStateHeader,
    pub states: *const CreatureState,
}

/// `LnkRezView`: the header by value, the three row arrays by borrow from the client's rez
/// rows - valid until the next poll or close, exactly like the tick's.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RezView {
    pub rez: Rez,
    pub vertices: *const RezVertex,
    pub triangles: *const RezTriangle,
    pub materials: *const RezMaterial,
}

/// `LnkProprioceptionView`: the header by value, the contacts borrowed until the next poll.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProprioceptionView {
    pub proprioception: Proprioception,
    pub contacts: *const Contact,
}

/// The union behind `LnkMessageView.as`. Every member is plain old data; reading the member the
/// `type` byte names is the C side's contract, mirrored in the tests here.
#[repr(C)]
#[derive(Clone, Copy)]
pub union MessageViewPayload {
    pub welcome: Welcome,
    pub tick_state: TickStateView,
    pub event: Event,
    pub derez: Derez,
    pub ping: Ping,
    pub pong: Pong,
    pub hello: Hello,
    pub actions: Actions,
    pub rez: RezView,
    pub proprioception: ProprioceptionView,
}

/// `LnkMessageView`, field for field.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MessageView {
    pub message_type: u8,
    pub reserved0: [u8; 7],
    pub payload: MessageViewPayload,
}

/// `LnkClientVTable`, field for field and in the header's exact order — the order is the ABI.
#[repr(C)]
pub struct LnkClientVTable {
    pub vtable_bytes: u32,
    pub abi_version: u32,
    pub protocol_version: extern "C" fn() -> u32,
    pub protocol_fingerprint: extern "C" fn(out_fingerprint: *mut u8),
    pub world_fingerprint: extern "C" fn(definition: *const WorldDefinition) -> u64,
    pub connect: extern "C" fn(
        address_utf8: *const c_char,
        role: u8,
        world_fingerprint: u64,
        timeout_milliseconds: u32,
        out_welcome: *mut Welcome,
        out_status: *mut LnkStatus,
        out_detail_utf8: *mut c_char,
        detail_capacity_bytes: u32,
    ) -> *mut LnkClient,
    pub poll: extern "C" fn(client: *mut LnkClient, out_message: *mut MessageView) -> LnkStatus,
    pub send_actions: extern "C" fn(client: *mut LnkClient, actions: *const Actions) -> LnkStatus,
    pub send_rez:
        extern "C" fn(client: *mut LnkClient, rez: *const Rez, vertices: *const RezVertex, triangles: *const RezTriangle, materials: *const RezMaterial) -> LnkStatus,
    pub send_ping: extern "C" fn(client: *mut LnkClient, nonce: u64) -> LnkStatus,
    pub send_pong: extern "C" fn(client: *mut LnkClient, nonce: u64) -> LnkStatus,
    pub flush: extern "C" fn(client: *mut LnkClient, out_everything_left: *mut u8) -> LnkStatus,
    pub close: extern "C" fn(client: *mut LnkClient),
    pub listen: extern "C" fn(port: u16, world_fingerprint: u64, out_status: *mut LnkStatus, out_detail_utf8: *mut c_char, detail_capacity_bytes: u32) -> *mut LnkServer,
    pub server_port: extern "C" fn(server: *mut LnkServer) -> u16,
    pub accept: extern "C" fn(
        server: *mut LnkServer,
        timeout_milliseconds: u32,
        out_hello: *mut Hello,
        out_status: *mut LnkStatus,
        out_detail_utf8: *mut c_char,
        detail_capacity_bytes: u32,
    ) -> *mut LnkClient,
    pub send_welcome: extern "C" fn(connection: *mut LnkClient, welcome: *const Welcome) -> LnkStatus,
    pub send_tick_state: extern "C" fn(connection: *mut LnkClient, header: *const TickStateHeader, states: *const CreatureState) -> LnkStatus,
    pub send_event: extern "C" fn(connection: *mut LnkClient, event: *const Event) -> LnkStatus,
    pub send_derez: extern "C" fn(connection: *mut LnkClient, derez: *const Derez) -> LnkStatus,
    pub send_proprioception: extern "C" fn(connection: *mut LnkClient, proprioception: *const Proprioception, contacts: *const Contact) -> LnkStatus,
    pub close_server: extern "C" fn(server: *mut LnkServer),
    pub record_open: extern "C" fn(
        path_utf8: *const c_char,
        world_fingerprint: u64,
        start_tick: u64,
        nominal_dt_seconds: f32,
        start_unix_seconds: u64,
        out_status: *mut LnkStatus,
        out_detail_utf8: *mut c_char,
        detail_capacity_bytes: u32,
    ) -> *mut LnkClient,
    pub replay_open: extern "C" fn(
        path_utf8: *const c_char,
        world_fingerprint: u64,
        out_welcome: *mut Welcome,
        out_status: *mut LnkStatus,
        out_detail_utf8: *mut c_char,
        detail_capacity_bytes: u32,
    ) -> *mut LnkClient,
}

static VTABLE: LnkClientVTable = LnkClientVTable {
    vtable_bytes: size_of::<LnkClientVTable>() as u32,
    abi_version: LNK_CLIENT_ABI_VERSION,
    protocol_version: protocol_version_impl,
    protocol_fingerprint: protocol_fingerprint_impl,
    world_fingerprint: world_fingerprint_impl,
    connect: connect_impl,
    poll: poll_impl,
    send_actions: send_actions_impl,
    send_rez: send_rez_impl,
    send_ping: send_ping_impl,
    send_pong: send_pong_impl,
    flush: flush_impl,
    close: close_impl,
    listen: listen_impl,
    server_port: server_port_impl,
    accept: accept_impl,
    send_welcome: send_welcome_impl,
    send_tick_state: send_tick_state_impl,
    send_event: send_event_impl,
    send_derez: send_derez_impl,
    send_proprioception: send_proprioception_impl,
    close_server: close_server_impl,
    record_open: record_open_impl,
    replay_open: replay_open_impl,
};

/// The library's one exported symbol. Kept camel-case to match the header and the flagship's
/// `tglGetProgramVTable` twin.
#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub extern "C" fn lnkGetClientVTable(abi_version: u32) -> *const LnkClientVTable {
    if abi_version == LNK_CLIENT_ABI_VERSION {
        &raw const VTABLE
    } else {
        std::ptr::null()
    }
}

/// Every boundary function runs inside this: a panic becomes the fallback value instead of an
/// unwind into a foreign runtime, which would be undefined behaviour rather than an error path.
fn guarded<R>(fallback: R, run: impl FnOnce() -> R) -> R {
    match catch_unwind(AssertUnwindSafe(run)) {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

fn status_of(error: &TransportError) -> LnkStatus {
    match error {
        TransportError::Io(_) | TransportError::WriteBufferOverflow => LNK_IO,
        TransportError::Frame(_) | TransportError::ActionsFromSpectator | TransportError::ProprioceptionAtServer => LNK_FRAME_REFUSED,
        TransportError::Refused { .. } => LNK_REFUSED,
        TransportError::Garbled(_) => LNK_GARBLED,
        TransportError::PeerClosed => LNK_PEER_CLOSED,
        TransportError::HandshakeTimedOut => LNK_HANDSHAKE_TIMED_OUT,
    }
}

fn detail_of(error: &TransportError) -> String {
    match error {
        TransportError::Refused { reason } => reason.clone(),
        TransportError::Io(kind) => format!("link: io error: {kind:?}"),
        TransportError::Frame(refusal) => format!("link: the wire contract refused a frame: {refusal:?}"),
        TransportError::Garbled(expectation) => format!("link: garbled handshake: {expectation}"),
        TransportError::PeerClosed => "link: the peer closed the connection".to_string(),
        TransportError::HandshakeTimedOut => "link: the handshake timed out".to_string(),
        TransportError::ActionsFromSpectator => "link: a spectator sent ACTIONS - the connection is over".to_string(),
        TransportError::ProprioceptionAtServer => "link: a client sent PROPRIOCEPTION, which only a server may - the connection is over".to_string(),
        TransportError::WriteBufferOverflow => "link: the write buffer overflowed - the peer is not reading".to_string(),
    }
}

/// Copy a NUL-terminated, possibly truncated UTF-8 line into the caller's buffer. A null buffer
/// or zero capacity means the caller declined the words, which is its right.
fn write_detail(detail: *mut c_char, capacity: u32, text: &str) {
    if detail.is_null() || capacity == 0 {
        return;
    }
    let room = (capacity - 1) as usize;
    let bytes = text.as_bytes();
    let take = bytes.len().min(room);
    // SAFETY: the caller handed this buffer and its capacity as a pair; writing at most
    // capacity-1 bytes plus the terminator stays inside what it promised.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), detail.cast::<u8>(), take);
        *detail.add(take) = 0;
    }
}

extern "C" fn protocol_version_impl() -> u32 {
    guarded(0, || PROTOCOL_VERSION)
}

extern "C" fn protocol_fingerprint_impl(out_fingerprint: *mut u8) {
    guarded((), || {
        if out_fingerprint.is_null() {
            return;
        }
        let fingerprint = recorded_fingerprint();
        // SAFETY: the header declares the out parameter as uint8_t[32]; the caller owns those
        // bytes and asked for them to be filled.
        unsafe {
            std::ptr::copy_nonoverlapping(fingerprint.as_ptr(), out_fingerprint, fingerprint.len());
        }
    });
}

extern "C" fn world_fingerprint_impl(definition: *const WorldDefinition) -> u64 {
    guarded(0, || {
        if definition.is_null() {
            // No definition, no world: zero is the fingerprint of nothing - the honest answer
            // to a null on a function without a status channel.
            return 0;
        }
        // SAFETY: non-null was just checked; WorldDefinition is plain old data, read by copy.
        world_fingerprint(&unsafe { *definition })
    })
}

extern "C" fn connect_impl(
    address_utf8: *const c_char,
    role: u8,
    world_fingerprint: u64,
    timeout_milliseconds: u32,
    out_welcome: *mut Welcome,
    out_status: *mut LnkStatus,
    out_detail_utf8: *mut c_char,
    detail_capacity_bytes: u32,
) -> *mut LnkClient {
    guarded(std::ptr::null_mut(), || {
        if out_status.is_null() {
            // Nowhere to say what went wrong: the only honest answer is nothing at all.
            return std::ptr::null_mut();
        }
        // Pre-set the panic verdict so an unwind caught by the guard leaves the truth behind.
        // SAFETY: out_status was just checked non-null and belongs to the caller.
        unsafe { *out_status = LNK_PANIC };

        let refuse = |status: LnkStatus, detail: &str| -> *mut LnkClient {
            // SAFETY: as above.
            unsafe { *out_status = status };
            write_detail(out_detail_utf8, detail_capacity_bytes, detail);
            std::ptr::null_mut()
        };

        if address_utf8.is_null() || out_welcome.is_null() {
            return refuse(LNK_BAD_ARGUMENT, "link: a null pointer is not an argument");
        }
        // SAFETY: the caller promised a NUL-terminated string; from_ptr reads to the NUL.
        let address = match unsafe { CStr::from_ptr(address_utf8) }.to_str() {
            Ok(address) => address,
            Err(_) => return refuse(LNK_BAD_ARGUMENT, "link: the address is not UTF-8"),
        };
        let role = match role {
            r if r == Role::Spectator as u8 => Role::Spectator,
            r if r == Role::CreatureHost as u8 => Role::CreatureHost,
            _ => return refuse(LNK_BAD_ARGUMENT, "link: role must be spectator (1) or creature host (2)"),
        };

        match connect(address, &local_hello(role, world_fingerprint), Duration::from_millis(u64::from(timeout_milliseconds))) {
            Ok((connection, welcome)) => {
                // SAFETY: out_welcome was checked non-null; Welcome is plain old data.
                unsafe {
                    *out_welcome = welcome;
                    *out_status = LNK_OK;
                }
                handle_for(End::Wire(connection))
            }
            Err(error) => refuse(status_of(&error), &detail_of(&error)),
        }
    })
}

/// The one place a handle pointer becomes a Rust borrow again.
///
/// # Safety
///
/// `client` must be a pointer returned by [`connect_impl`] and not yet passed to
/// [`close_impl`] — which is exactly the contract the header states.
unsafe fn inner_of<'a>(client: *mut LnkClient) -> &'a mut ClientInner {
    // SAFETY: delegated to the caller, per the function's contract above.
    unsafe { &mut *client.cast::<ClientInner>() }
}

extern "C" fn poll_impl(client: *mut LnkClient, out_message: *mut MessageView) -> LnkStatus {
    guarded(LNK_PANIC, || {
        if client.is_null() || out_message.is_null() {
            return LNK_BAD_ARGUMENT;
        }
        // SAFETY: non-null was just checked; validity is the header's stated contract.
        let inner = unsafe { inner_of(client) };

        let message = match inner.end.poll() {
            Ok(None) => return LNK_NOTHING_YET,
            Ok(Some(message)) => message,
            Err(error) => return status_of(&error),
        };

        let (message_type, payload) = match message {
            Message::Hello(hello) => (MessageType::Hello, MessageViewPayload { hello }),
            Message::Welcome(welcome) => (MessageType::Welcome, MessageViewPayload { welcome }),
            Message::TickState { header, states } => {
                inner.tick_rows = states;
                (
                    MessageType::TickState,
                    MessageViewPayload {
                        tick_state: TickStateView {
                            header,
                            states: inner.tick_rows.as_ptr(),
                        },
                    },
                )
            }
            Message::Actions(actions) => (MessageType::Actions, MessageViewPayload { actions }),
            Message::Rez {
                header,
                vertices,
                triangles,
                materials,
            } => {
                inner.rez_vertices = vertices;
                inner.rez_triangles = triangles;
                inner.rez_materials = materials;
                (
                    MessageType::Rez,
                    MessageViewPayload {
                        rez: RezView {
                            rez: header,
                            vertices: inner.rez_vertices.as_ptr(),
                            triangles: inner.rez_triangles.as_ptr(),
                            materials: inner.rez_materials.as_ptr(),
                        },
                    },
                )
            }
            Message::Proprioception { header, contacts } => {
                inner.contacts = contacts;
                (
                    MessageType::Proprioception,
                    MessageViewPayload {
                        proprioception: ProprioceptionView {
                            proprioception: header,
                            contacts: inner.contacts.as_ptr(),
                        },
                    },
                )
            }
            Message::Event(event) => (MessageType::Event, MessageViewPayload { event }),
            Message::Derez(derez) => (MessageType::Derez, MessageViewPayload { derez }),
            Message::Ping(ping) => (MessageType::Ping, MessageViewPayload { ping }),
            Message::Pong(pong) => (MessageType::Pong, MessageViewPayload { pong }),
            Message::Bye => (MessageType::Bye, MessageViewPayload { ping: Ping { nonce: 0 } }),
        };

        // SAFETY: out_message was checked non-null; MessageView is plain old data.
        unsafe {
            (*out_message).message_type = message_type as u8;
            (*out_message).reserved0 = [0; 7];
            (*out_message).payload = payload;
        }
        LNK_OK
    })
}

fn queue_on(client: *mut LnkClient, message: &Message) -> LnkStatus {
    if client.is_null() {
        return LNK_BAD_ARGUMENT;
    }
    // SAFETY: non-null was just checked; validity is the header's stated contract.
    let inner = unsafe { inner_of(client) };
    match inner.end.queue(message) {
        Ok(()) => LNK_OK,
        Err(_) => LNK_BAD_ARGUMENT,
    }
}

extern "C" fn send_actions_impl(client: *mut LnkClient, actions: *const Actions) -> LnkStatus {
    guarded(LNK_PANIC, || {
        if actions.is_null() || client.is_null() {
            return LNK_BAD_ARGUMENT;
        }
        // SAFETY: non-null was just checked; validity is the header's stated contract.
        let inner = unsafe { inner_of(client) };
        if !inner.end.may_send_intents() {
            // A spectator never sends ACTIONS, and the refusal starts at the sending half so an
            // honest client cannot even stage the violation the server end would hang up on.
            return LNK_BAD_ARGUMENT;
        }
        // SAFETY: non-null was just checked; Actions is plain old data, read by copy.
        let actions = unsafe { *actions };
        match inner.end.queue(&Message::Actions(actions)) {
            Ok(()) => LNK_OK,
            Err(_) => LNK_BAD_ARGUMENT,
        }
    })
}

extern "C" fn send_rez_impl(
    client: *mut LnkClient,
    rez: *const Rez,
    vertices: *const RezVertex,
    triangles: *const RezTriangle,
    materials: *const RezMaterial,
) -> LnkStatus {
    guarded(LNK_PANIC, || {
        if rez.is_null() || client.is_null() {
            return LNK_BAD_ARGUMENT;
        }
        // SAFETY: non-null was just checked; validity is the header's stated contract.
        let inner = unsafe { inner_of(client) };
        if !inner.end.may_send_intents() {
            // A spectator never sends REZ either: the same refusal, at the same sending half.
            return LNK_BAD_ARGUMENT;
        }
        // SAFETY: non-null was just checked; Rez is plain old data, read by copy.
        let header = unsafe { *rez };

        // Every count is judged against its cap before a single row is read - the wire's
        // validate-before-copy rule applied to our own caller, so a lying count can never make
        // this library read past the arrays it was handed.
        if header.vertex_count > REZ_MAX_VERTICES || header.triangle_count > REZ_MAX_TRIANGLES || header.material_count > REZ_MAX_MATERIALS {
            return LNK_BAD_ARGUMENT;
        }
        if (header.vertex_count > 0 && vertices.is_null()) || (header.triangle_count > 0 && triangles.is_null()) || (header.material_count > 0 && materials.is_null()) {
            return LNK_BAD_ARGUMENT;
        }
        /// Rows by copy, or none: a zero count never touches the pointer, so a bodiless REZ
        /// may pass NULL for every array.
        fn rows<T: Copy>(pointer: *const T, count: u32) -> Vec<T> {
            if count == 0 {
                Vec::new()
            } else {
                // SAFETY: the pointer is non-null (checked by the caller) and the caller
                // promised `count` rows; the count was capped above.
                unsafe { std::slice::from_raw_parts(pointer, count as usize) }.to_vec()
            }
        }
        let message = Message::Rez {
            header,
            vertices: rows(vertices, header.vertex_count),
            triangles: rows(triangles, header.triangle_count),
            materials: rows(materials, header.material_count),
        };
        // The codec judges the rest - indices in range, floats finite - and a refusal there is
        // the caller's bad argument, not a wire failure.
        match inner.end.queue(&message) {
            Ok(()) => LNK_OK,
            Err(_) => LNK_BAD_ARGUMENT,
        }
    })
}

extern "C" fn send_ping_impl(client: *mut LnkClient, nonce: u64) -> LnkStatus {
    guarded(LNK_PANIC, || queue_on(client, &Message::Ping(Ping { nonce })))
}

extern "C" fn send_pong_impl(client: *mut LnkClient, nonce: u64) -> LnkStatus {
    guarded(LNK_PANIC, || queue_on(client, &Message::Pong(Pong { nonce })))
}

extern "C" fn flush_impl(client: *mut LnkClient, out_everything_left: *mut u8) -> LnkStatus {
    guarded(LNK_PANIC, || {
        if client.is_null() || out_everything_left.is_null() {
            return LNK_BAD_ARGUMENT;
        }
        // SAFETY: non-null was just checked; validity is the header's stated contract.
        let inner = unsafe { inner_of(client) };
        match inner.end.flush() {
            Ok(done) => {
                // SAFETY: out_everything_left was checked non-null.
                unsafe { *out_everything_left = u8::from(done) };
                LNK_OK
            }
            Err(error) => status_of(&error),
        }
    })
}

extern "C" fn listen_impl(port: u16, world_fingerprint: u64, out_status: *mut LnkStatus, out_detail_utf8: *mut c_char, detail_capacity_bytes: u32) -> *mut LnkServer {
    guarded(std::ptr::null_mut(), || {
        if out_status.is_null() {
            return std::ptr::null_mut();
        }
        // SAFETY: out_status was just checked non-null and belongs to the caller. Pre-set the
        // panic verdict so an unwind caught by the guard leaves the truth behind.
        unsafe { *out_status = LNK_PANIC };

        match listen(port) {
            Ok(listener) => {
                // SAFETY: as above.
                unsafe { *out_status = LNK_OK };
                Box::into_raw(Box::new(ServerInner { listener, world_fingerprint })).cast::<LnkServer>()
            }
            Err(error) => {
                // SAFETY: as above.
                unsafe { *out_status = status_of(&error) };
                write_detail(out_detail_utf8, detail_capacity_bytes, &detail_of(&error));
                std::ptr::null_mut()
            }
        }
    })
}

/// The one place a server pointer becomes a Rust borrow again.
///
/// # Safety
///
/// `server` must be a pointer returned by [`listen_impl`] and not yet passed to
/// [`close_server_impl`] — the contract the header states.
unsafe fn server_of<'a>(server: *mut LnkServer) -> &'a mut ServerInner {
    // SAFETY: delegated to the caller, per the function's contract above.
    unsafe { &mut *server.cast::<ServerInner>() }
}

extern "C" fn server_port_impl(server: *mut LnkServer) -> u16 {
    guarded(0, || {
        if server.is_null() {
            return 0;
        }
        // SAFETY: non-null was just checked; validity is the header's stated contract.
        let inner = unsafe { server_of(server) };
        inner.listener.port().unwrap_or(0)
    })
}

extern "C" fn accept_impl(
    server: *mut LnkServer,
    timeout_milliseconds: u32,
    out_hello: *mut Hello,
    out_status: *mut LnkStatus,
    out_detail_utf8: *mut c_char,
    detail_capacity_bytes: u32,
) -> *mut LnkClient {
    guarded(std::ptr::null_mut(), || {
        if out_status.is_null() {
            return std::ptr::null_mut();
        }
        // SAFETY: out_status was just checked non-null; pre-set the panic verdict.
        unsafe { *out_status = LNK_PANIC };

        let refuse = |status: LnkStatus, detail: &str| -> *mut LnkClient {
            // SAFETY: as above.
            unsafe { *out_status = status };
            write_detail(out_detail_utf8, detail_capacity_bytes, detail);
            std::ptr::null_mut()
        };

        if server.is_null() || out_hello.is_null() {
            return refuse(LNK_BAD_ARGUMENT, "link: a null pointer is not an argument");
        }
        // SAFETY: non-null was just checked; validity is the header's stated contract.
        let inner = unsafe { server_of(server) };

        let stream = match inner.listener.knock() {
            Ok(None) => {
                // SAFETY: as above.
                unsafe { *out_status = LNK_NOTHING_YET };
                return std::ptr::null_mut();
            }
            Ok(Some(stream)) => stream,
            Err(error) => return refuse(status_of(&error), &detail_of(&error)),
        };

        match crate::transport::accept(stream, Duration::from_millis(u64::from(timeout_milliseconds)), inner.world_fingerprint) {
            Ok((connection, hello)) => {
                // SAFETY: out_hello was checked non-null; Hello is plain old data.
                unsafe {
                    *out_hello = hello;
                    *out_status = LNK_OK;
                }
                handle_for(End::Wire(connection))
            }
            Err(error) => refuse(status_of(&error), &detail_of(&error)),
        }
    })
}

extern "C" fn send_welcome_impl(connection: *mut LnkClient, welcome: *const Welcome) -> LnkStatus {
    guarded(LNK_PANIC, || {
        if welcome.is_null() {
            return LNK_BAD_ARGUMENT;
        }
        // SAFETY: non-null was just checked; Welcome is plain old data, read by copy.
        let welcome = unsafe { *welcome };
        queue_on(connection, &Message::Welcome(welcome))
    })
}

extern "C" fn send_tick_state_impl(connection: *mut LnkClient, header: *const TickStateHeader, states: *const CreatureState) -> LnkStatus {
    guarded(LNK_PANIC, || {
        if header.is_null() {
            return LNK_BAD_ARGUMENT;
        }
        // SAFETY: non-null was just checked; TickStateHeader is plain old data, read by copy.
        let header = unsafe { *header };

        // The count is judged before a single row is read, so a lying count cannot make this
        // library read past the caller's array — the wire's validate-before-copy rule, applied
        // to our own caller with exactly the trust a stranger would get.
        if header.creature_count > crate::protocol::TICK_STATE_MAX_CREATURES {
            return LNK_BAD_ARGUMENT;
        }
        if header.creature_count > 0 && states.is_null() {
            return LNK_BAD_ARGUMENT;
        }
        let rows = if header.creature_count == 0 {
            Vec::new()
        } else {
            // SAFETY: states is non-null and the caller promised creature_count rows; the count
            // was capped above, so at most TICK_STATE_MAX_CREATURES rows are read.
            unsafe { std::slice::from_raw_parts(states, header.creature_count as usize) }.to_vec()
        };
        queue_on(connection, &Message::TickState { header, states: rows })
    })
}

extern "C" fn send_event_impl(connection: *mut LnkClient, event: *const Event) -> LnkStatus {
    guarded(LNK_PANIC, || {
        if event.is_null() {
            return LNK_BAD_ARGUMENT;
        }
        // SAFETY: non-null was just checked; Event is plain old data, read by copy.
        let event = unsafe { *event };
        queue_on(connection, &Message::Event(event))
    })
}

extern "C" fn send_derez_impl(connection: *mut LnkClient, derez: *const Derez) -> LnkStatus {
    guarded(LNK_PANIC, || {
        if derez.is_null() {
            return LNK_BAD_ARGUMENT;
        }
        // SAFETY: non-null was just checked; Derez is plain old data, read by copy.
        let derez = unsafe { *derez };
        queue_on(connection, &Message::Derez(derez))
    })
}

extern "C" fn send_proprioception_impl(connection: *mut LnkClient, proprioception: *const Proprioception, contacts: *const Contact) -> LnkStatus {
    guarded(LNK_PANIC, || {
        if proprioception.is_null() || connection.is_null() {
            return LNK_BAD_ARGUMENT;
        }
        // SAFETY: non-null was just checked; validity is the header's stated contract.
        let inner = unsafe { inner_of(connection) };
        if !inner.end.may_send_proprioception() {
            // The letter flows one way: a client-held connection never sends it, and the refusal
            // starts at the sending half, exactly as the spectator's does.
            return LNK_BAD_ARGUMENT;
        }
        // SAFETY: non-null was just checked; Proprioception is plain old data, read by copy.
        let header = unsafe { *proprioception };
        if header.contact_count > CONTACTS_MAX {
            return LNK_BAD_ARGUMENT;
        }
        if header.contact_count > 0 && contacts.is_null() {
            return LNK_BAD_ARGUMENT;
        }
        let rows = if header.contact_count == 0 {
            Vec::new()
        } else {
            // SAFETY: contacts is non-null and the caller promised contact_count rows; the
            // count was capped above.
            unsafe { std::slice::from_raw_parts(contacts, header.contact_count as usize) }.to_vec()
        };
        match inner.end.queue(&Message::Proprioception { header, contacts: rows }) {
            Ok(()) => LNK_OK,
            Err(_) => LNK_BAD_ARGUMENT,
        }
    })
}

/// A fresh handle around an end, with empty row storage.
fn handle_for(end: End) -> *mut LnkClient {
    Box::into_raw(Box::new(ClientInner {
        end,
        tick_rows: Vec::new(),
        rez_vertices: Vec::new(),
        rez_triangles: Vec::new(),
        rez_materials: Vec::new(),
        contacts: Vec::new(),
    }))
    .cast::<LnkClient>()
}

/// The path argument, as every path-taking entry reads it: NUL-terminated UTF-8, or refused.
///
/// # Safety
///
/// `path_utf8` must be null or point at a NUL-terminated string, per the header's contract.
unsafe fn path_of<'a>(path_utf8: *const c_char) -> Result<&'a str, &'static str> {
    if path_utf8.is_null() {
        return Err("link: a null pointer is not a path");
    }
    // SAFETY: the caller promised a NUL-terminated string; from_ptr reads to the NUL.
    unsafe { CStr::from_ptr(path_utf8) }.to_str().map_err(|_| "link: the path is not UTF-8")
}

extern "C" fn record_open_impl(
    path_utf8: *const c_char,
    world_fingerprint: u64,
    start_tick: u64,
    nominal_dt_seconds: f32,
    start_unix_seconds: u64,
    out_status: *mut LnkStatus,
    out_detail_utf8: *mut c_char,
    detail_capacity_bytes: u32,
) -> *mut LnkClient {
    guarded(std::ptr::null_mut(), || {
        if out_status.is_null() {
            return std::ptr::null_mut();
        }
        // SAFETY: out_status was just checked non-null; pre-set the panic verdict.
        unsafe { *out_status = LNK_PANIC };
        let refuse = |status: LnkStatus, detail: &str| -> *mut LnkClient {
            // SAFETY: as above.
            unsafe { *out_status = status };
            write_detail(out_detail_utf8, detail_capacity_bytes, detail);
            std::ptr::null_mut()
        };
        // SAFETY: the header's contract for path_utf8.
        let path = match unsafe { path_of(path_utf8) } {
            Ok(path) => path,
            Err(words) => return refuse(LNK_BAD_ARGUMENT, words),
        };
        match Recorder::create(std::path::Path::new(path), world_fingerprint, start_tick, nominal_dt_seconds, start_unix_seconds) {
            Ok(recorder) => {
                // SAFETY: as above.
                unsafe { *out_status = LNK_OK };
                handle_for(End::Recording(recorder))
            }
            Err(error) => refuse(status_of(&error), &detail_of(&error)),
        }
    })
}

extern "C" fn replay_open_impl(
    path_utf8: *const c_char,
    world_fingerprint: u64,
    out_welcome: *mut Welcome,
    out_status: *mut LnkStatus,
    out_detail_utf8: *mut c_char,
    detail_capacity_bytes: u32,
) -> *mut LnkClient {
    guarded(std::ptr::null_mut(), || {
        if out_status.is_null() {
            return std::ptr::null_mut();
        }
        // SAFETY: out_status was just checked non-null; pre-set the panic verdict.
        unsafe { *out_status = LNK_PANIC };
        let refuse = |status: LnkStatus, detail: &str| -> *mut LnkClient {
            // SAFETY: as above.
            unsafe { *out_status = status };
            write_detail(out_detail_utf8, detail_capacity_bytes, detail);
            std::ptr::null_mut()
        };
        if out_welcome.is_null() {
            return refuse(LNK_BAD_ARGUMENT, "link: a null pointer is not an argument");
        }
        // SAFETY: the header's contract for path_utf8.
        let path = match unsafe { path_of(path_utf8) } {
            Ok(path) => path,
            Err(words) => return refuse(LNK_BAD_ARGUMENT, words),
        };
        match Replayer::open(std::path::Path::new(path), world_fingerprint) {
            Ok(replay) => {
                let header = *replay.header();
                // SAFETY: out_welcome was checked non-null; Welcome is plain old data.
                unsafe {
                    *out_welcome = Welcome {
                        current_tick: header.start_tick,
                        nominal_dt_seconds: header.nominal_dt_seconds,
                        client_id: 0,
                        world_fingerprint: header.world_fingerprint,
                    };
                    *out_status = LNK_OK;
                }
                handle_for(End::Replay(replay))
            }
            Err(error) => refuse(status_of(&error), &detail_of(&error)),
        }
    })
}

extern "C" fn close_server_impl(server: *mut LnkServer) {
    guarded((), || {
        if server.is_null() {
            return;
        }
        // SAFETY: the header's contract — a pointer from listen, not yet closed — and from here
        // the box owns it again, so it is freed exactly once.
        drop(unsafe { Box::from_raw(server.cast::<ServerInner>()) });
    });
}

extern "C" fn close_impl(client: *mut LnkClient) {
    guarded((), || {
        if client.is_null() {
            return;
        }
        // SAFETY: the header's contract - a pointer from connect, not yet closed - and from
        // here the box owns it again, so it is freed exactly once.
        let inner = unsafe { Box::from_raw(client.cast::<ClientInner>()) };
        inner.end.close();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::Message;
    use crate::protocol::{EventKind, TICK_STATE_MAX_CREATURES, TickStateHeader};
    use crate::transport::accept;
    use std::ffi::CString;
    use std::net::TcpListener;

    const PATIENCE: Duration = Duration::from_secs(5);
    /// The one world every test here lives in - a fingerprint, not a definition, because the
    /// transport only ever compares the number.
    const WORLD: u64 = 0x5EED_0F7E_601D;

    fn vtable() -> &'static LnkClientVTable {
        let table = lnkGetClientVTable(LNK_CLIENT_ABI_VERSION);
        // SAFETY: as_ref is the null check and the dereference in one inseparable expression;
        // the non-null pointer is the static VTABLE, alive for the program's whole life.
        unsafe { table.as_ref() }.expect("the library must satisfy its own version")
    }

    #[test]
    fn the_vtable_refuses_every_version_but_its_own() {
        assert!(lnkGetClientVTable(0).is_null());
        assert!(lnkGetClientVTable(LNK_CLIENT_ABI_VERSION + 1).is_null());
        let table = vtable();
        assert_eq!(table.vtable_bytes as usize, size_of::<LnkClientVTable>());
        assert_eq!(table.abi_version, LNK_CLIENT_ABI_VERSION);
        assert_eq!((table.protocol_version)(), PROTOCOL_VERSION);
        let mut fingerprint = [0u8; 32];
        (table.protocol_fingerprint)(fingerprint.as_mut_ptr());
        assert_eq!(fingerprint, recorded_fingerprint());
    }

    #[test]
    fn null_arguments_are_refused_not_dereferenced() {
        let table = vtable();
        let mut view = unsafe { std::mem::zeroed::<MessageView>() };
        assert_eq!((table.poll)(std::ptr::null_mut(), &raw mut view), LNK_BAD_ARGUMENT);
        assert_eq!((table.send_actions)(std::ptr::null_mut(), std::ptr::null()), LNK_BAD_ARGUMENT);
        assert_eq!(
            (table.send_rez)(std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null()),
            LNK_BAD_ARGUMENT
        );
        assert_eq!((table.send_ping)(std::ptr::null_mut(), 1), LNK_BAD_ARGUMENT);
        let mut left = 0u8;
        assert_eq!((table.flush)(std::ptr::null_mut(), &raw mut left), LNK_BAD_ARGUMENT);
        (table.close)(std::ptr::null_mut());

        let mut welcome = unsafe { std::mem::zeroed::<Welcome>() };
        let mut status: LnkStatus = -1;
        let client = (table.connect)(std::ptr::null(), 2, WORLD, 100, &raw mut welcome, &raw mut status, std::ptr::null_mut(), 0);
        assert!(client.is_null());
        assert_eq!(status, LNK_BAD_ARGUMENT);

        let address = CString::new("127.0.0.1:1").expect("address literal");
        let client = (table.connect)(address.as_ptr(), 7, WORLD, 100, &raw mut welcome, &raw mut status, std::ptr::null_mut(), 0);
        assert!(client.is_null());
        assert_eq!(status, LNK_BAD_ARGUMENT, "a role that is not a role is refused before any socket");
    }

    #[test]
    fn a_c_shaped_caller_gets_the_whole_conversation() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback");
        let address = listener.local_addr().expect("address");

        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            let (mut connection, hello) = accept(stream, PATIENCE, WORLD).expect("handshake");
            assert_eq!(hello.role, Role::CreatureHost as u8);

            connection
                .queue(&Message::Welcome(Welcome {
                    current_tick: 100,
                    nominal_dt_seconds: 0.03125,
                    client_id: 9,
                    world_fingerprint: WORLD,
                }))
                .expect("queue WELCOME");
            let rows = vec![
                CreatureState {
                    creature_id: 0,
                    position: [0.0, 1.0, 2.0],
                    yaw: 0.1,
                    velocity: [0.0, 0.0, 0.0],
                    yaw_rate: 0.0,
                    vocalisation: 0.0,
                },
                CreatureState {
                    creature_id: 1,
                    position: [3.0, 4.0, 5.0],
                    yaw: 0.2,
                    velocity: [1.0, 0.0, 0.0],
                    yaw_rate: 0.5,
                    vocalisation: 0.75,
                },
            ];
            connection
                .queue(&Message::TickState {
                    header: TickStateHeader {
                        tick: 100,
                        creature_count: 2,
                        reserved0: [0; 4],
                    },
                    states: rows,
                })
                .expect("queue TICK_STATE");
            assert!(connection.flush().expect("flush"));

            // Now the client's turn: ACTIONS then PING, answered with a PONG.
            let deadline = std::time::Instant::now() + PATIENCE;
            let mut heard_actions = false;
            loop {
                match connection.poll().expect("server poll") {
                    Some(Message::Actions(actions)) => {
                        assert_eq!(actions.tick, 101);
                        assert_eq!(actions.creature_id, 1);
                        heard_actions = true;
                    }
                    Some(Message::Ping(ping)) => {
                        assert!(heard_actions, "coalesced order: ACTIONS was queued first");
                        connection.queue(&Message::Pong(Pong { nonce: ping.nonce })).expect("queue PONG");
                        assert!(connection.flush().expect("flush PONG"));
                        break;
                    }
                    Some(other) => panic!("unexpected {other:?}"),
                    None => {
                        assert!(std::time::Instant::now() < deadline, "client frames never arrived");
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
            }
        });

        let table = vtable();
        let address = CString::new(address.to_string()).expect("address string");
        let mut welcome = unsafe { std::mem::zeroed::<Welcome>() };
        let mut status: LnkStatus = -1;
        let mut detail = [0i8; 256];
        let client = (table.connect)(
            address.as_ptr(),
            Role::CreatureHost as u8,
            WORLD,
            5_000,
            &raw mut welcome,
            &raw mut status,
            detail.as_mut_ptr().cast::<c_char>(),
            detail.len() as u32,
        );
        assert_eq!(status, LNK_OK);
        assert!(!client.is_null());
        assert_eq!(welcome.current_tick, 100);
        assert_eq!(welcome.client_id, 9);

        let mut view = unsafe { std::mem::zeroed::<MessageView>() };
        let deadline = std::time::Instant::now() + PATIENCE;
        loop {
            match (table.poll)(client, &raw mut view) {
                LNK_NOTHING_YET => {
                    assert!(std::time::Instant::now() < deadline, "TICK_STATE never arrived");
                    std::thread::sleep(Duration::from_millis(1));
                }
                LNK_OK => break,
                other => panic!("poll answered {other}"),
            }
        }
        assert_eq!(view.message_type, MessageType::TickState as u8);
        // SAFETY: the type byte names the union member, and the rows stay valid until the next
        // poll — the exact contract the header states for the C side.
        let (header, second_row) = unsafe {
            let tick_state = view.payload.tick_state;
            (tick_state.header, *tick_state.states.add(1))
        };
        assert_eq!(header.tick, 100);
        assert_eq!(header.creature_count, 2);
        assert_eq!(second_row.creature_id, 1);
        assert_eq!(second_row.vocalisation, 0.75);

        let actions = Actions {
            tick: 101,
            creature_id: 1,
            desired_forward_speed: 1.5,
            desired_turn_rate: -0.25,
            vocalisation_strength: 0.0,
            previous_forward_speed: 1.25,
            previous_turn_rate: -0.5,
            previous_vocalisation: 0.0,
            reserved0: [0; 4],
        };
        assert_eq!((table.send_actions)(client, &raw const actions), LNK_OK);
        assert_eq!((table.send_ping)(client, 0xC0FFEE), LNK_OK);
        let mut everything_left = 0u8;
        assert_eq!((table.flush)(client, &raw mut everything_left), LNK_OK);
        assert_eq!(everything_left, 1);

        let deadline = std::time::Instant::now() + PATIENCE;
        loop {
            match (table.poll)(client, &raw mut view) {
                LNK_NOTHING_YET => {
                    assert!(std::time::Instant::now() < deadline, "PONG never arrived");
                    std::thread::sleep(Duration::from_millis(1));
                }
                LNK_OK => break,
                other => panic!("poll answered {other}"),
            }
        }
        assert_eq!(view.message_type, MessageType::Pong as u8);
        // SAFETY: the type byte names the union member.
        assert_eq!(unsafe { view.payload.pong.nonce }, 0xC0FFEE);

        (table.close)(client);
        server.join().expect("server thread");
    }

    #[test]
    fn a_refusal_reaches_the_c_caller_in_words() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback");
        let address = listener.local_addr().expect("address");

        let server = std::thread::spawn(move || {
            use std::io::{Read, Write};
            let (mut stream, _) = listener.accept().expect("accept");
            let mut opening = [0u8; 4 + 3 + 48];
            stream.read_exact(&mut opening).expect("magic and HELLO");
            stream.write_all(b"link: no vacancy tonight\n").expect("refusal");
        });

        let table = vtable();
        let address = CString::new(address.to_string()).expect("address string");
        let mut welcome = unsafe { std::mem::zeroed::<Welcome>() };
        let mut status: LnkStatus = -1;
        let mut detail = [0i8; 256];
        let client = (table.connect)(
            address.as_ptr(),
            Role::Spectator as u8,
            WORLD,
            5_000,
            &raw mut welcome,
            &raw mut status,
            detail.as_mut_ptr().cast::<c_char>(),
            detail.len() as u32,
        );
        assert!(client.is_null());
        assert_eq!(status, LNK_REFUSED);
        let words = unsafe { CStr::from_ptr(detail.as_ptr().cast::<c_char>()) }.to_string_lossy().to_string();
        assert!(words.contains("no vacancy"), "the refusal must arrive verbatim, got: {words}");
        server.join().expect("server thread");
    }

    #[test]
    fn old_versions_are_history() {
        for old in 1..LNK_CLIENT_ABI_VERSION {
            assert!(
                lnkGetClientVTable(old).is_null(),
                "ABI {old} must be refused now that {LNK_CLIENT_ABI_VERSION} exists"
            );
        }
    }

    #[test]
    fn a_spectator_cannot_stage_actions_and_a_creature_host_can() {
        let table = vtable();
        let mut status: LnkStatus = -1;
        let server = (table.listen)(0, WORLD, &raw mut status, std::ptr::null_mut(), 0);
        assert_eq!(status, LNK_OK);
        let port = (table.server_port)(server);

        for (role, expected) in [(Role::Spectator, LNK_BAD_ARGUMENT), (Role::CreatureHost, LNK_OK)] {
            let client_thread = std::thread::spawn(move || {
                let table = vtable();
                let address = CString::new(format!("127.0.0.1:{port}")).expect("address");
                let mut welcome = unsafe { std::mem::zeroed::<Welcome>() };
                let mut status: LnkStatus = -1;
                let client = (table.connect)(address.as_ptr(), role as u8, WORLD, 5_000, &raw mut welcome, &raw mut status, std::ptr::null_mut(), 0);
                assert_eq!(status, LNK_OK);

                let actions = Actions {
                    tick: 7,
                    creature_id: 1,
                    desired_forward_speed: 1.0,
                    desired_turn_rate: 0.0,
                    vocalisation_strength: 0.0,
                    previous_forward_speed: 0.5,
                    previous_turn_rate: 0.0,
                    previous_vocalisation: 0.0,
                    reserved0: [0; 4],
                };
                let verdict = (table.send_actions)(client, &raw const actions);
                let rez = bodiless_rez(1);
                let rez_verdict = (table.send_rez)(client, &raw const rez, std::ptr::null(), std::ptr::null(), std::ptr::null());
                assert_eq!(rez_verdict, verdict, "REZ and ACTIONS share one role rule at the sending half");
                (table.close)(client);
                verdict
            });

            let mut hello = unsafe { std::mem::zeroed::<Hello>() };
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            let connection = loop {
                let knock = (table.accept)(server, 5_000, &raw mut hello, &raw mut status, std::ptr::null_mut(), 0);
                if !knock.is_null() {
                    break knock;
                }
                assert_eq!(status, LNK_NOTHING_YET);
                assert!(std::time::Instant::now() < deadline, "nobody knocked");
                std::thread::sleep(std::time::Duration::from_millis(1));
            };
            assert_eq!(hello.role, role as u8);

            // The caller sends WELCOME itself, promptly - the client's connect blocks on it.
            let welcome = Welcome {
                current_tick: 1,
                nominal_dt_seconds: 0.031_25,
                client_id: 1,
                world_fingerprint: WORLD,
            };
            assert_eq!((table.send_welcome)(connection, &raw const welcome), LNK_OK);
            let mut everything_left = 0u8;
            assert_eq!((table.flush)(connection, &raw mut everything_left), LNK_OK);

            let verdict = client_thread.join().expect("client thread");
            assert_eq!(verdict, expected, "role {role:?} staging ACTIONS answered the wrong status");
            (table.close)(connection);
        }
        (table.close_server)(server);
    }

    #[test]
    fn the_server_half_carries_a_whole_world_tick() {
        let table = vtable();
        let mut status: LnkStatus = -1;
        let server = (table.listen)(0, WORLD, &raw mut status, std::ptr::null_mut(), 0);
        assert_eq!(status, LNK_OK);
        assert!(!server.is_null());
        let port = (table.server_port)(server);
        assert_ne!(port, 0, "listen(0) answers with the port the operating system granted");

        // Nobody has knocked yet: the server's loop hears nothing and moves on.
        let mut hello = unsafe { std::mem::zeroed::<Hello>() };
        let knock = (table.accept)(server, 1_000, &raw mut hello, &raw mut status, std::ptr::null_mut(), 0);
        assert!(knock.is_null());
        assert_eq!(status, LNK_NOTHING_YET);

        let client_thread = std::thread::spawn(move || {
            let table = vtable();
            let address = CString::new(format!("127.0.0.1:{port}")).expect("address");
            let mut welcome = unsafe { std::mem::zeroed::<Welcome>() };
            let mut status: LnkStatus = -1;
            let client = (table.connect)(
                address.as_ptr(),
                Role::Spectator as u8,
                WORLD,
                5_000,
                &raw mut welcome,
                &raw mut status,
                std::ptr::null_mut(),
                0,
            );
            assert_eq!(status, LNK_OK);
            assert!(!client.is_null());
            assert_eq!(welcome.current_tick, 900);
            assert_eq!(welcome.client_id, 4);

            let mut view = unsafe { std::mem::zeroed::<MessageView>() };
            let mut seen = Vec::new();
            let deadline = std::time::Instant::now() + PATIENCE;
            while seen.len() < 3 {
                match (table.poll)(client, &raw mut view) {
                    LNK_NOTHING_YET => {
                        assert!(std::time::Instant::now() < deadline, "the world tick never arrived");
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    LNK_OK => {
                        seen.push(view.message_type);
                        if seen.len() == 1 {
                            assert_eq!(view.message_type, MessageType::TickState as u8);
                            // SAFETY: the type byte names the union member; the rows stay valid
                            // until the next poll, per the header's contract.
                            let tick_state = unsafe { view.payload.tick_state };
                            assert_eq!(tick_state.header.tick, 900);
                            assert_eq!(tick_state.header.creature_count, 2);
                            assert_eq!(unsafe { (*tick_state.states.add(1)).creature_id }, 7);
                        }
                    }
                    other => panic!("poll answered {other}"),
                }
            }
            let expected = vec![MessageType::TickState as u8, MessageType::Event as u8, MessageType::Derez as u8];
            assert_eq!(seen, expected, "one coalesced write, three frames, in order");
            (table.close)(client);
        });

        let deadline = std::time::Instant::now() + PATIENCE;
        let connection = loop {
            let connection = (table.accept)(server, 5_000, &raw mut hello, &raw mut status, std::ptr::null_mut(), 0);
            if !connection.is_null() {
                break connection;
            }
            assert_eq!(status, LNK_NOTHING_YET, "an arriving knock must not fail");
            assert!(std::time::Instant::now() < deadline, "the knock never arrived");
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(status, LNK_OK);
        assert_eq!(hello.role, Role::Spectator as u8);
        assert_eq!(hello.protocol_version, PROTOCOL_VERSION);

        let welcome = Welcome {
            current_tick: 900,
            nominal_dt_seconds: 0.031_25,
            client_id: 4,
            world_fingerprint: WORLD,
        };
        assert_eq!((table.send_welcome)(connection, &raw const welcome), LNK_OK);
        let rows = [
            CreatureState {
                creature_id: 3,
                position: [0.0, 1.0, 2.0],
                yaw: 0.0,
                velocity: [0.0; 3],
                yaw_rate: 0.0,
                vocalisation: 0.0,
            },
            CreatureState {
                creature_id: 7,
                position: [5.0, 1.0, 2.0],
                yaw: 0.5,
                velocity: [1.0, 0.0, 0.0],
                yaw_rate: 0.1,
                vocalisation: 0.9,
            },
        ];
        let header = TickStateHeader {
            tick: 900,
            creature_count: 2,
            reserved0: [0; 4],
        };
        assert_eq!((table.send_tick_state)(connection, &raw const header, rows.as_ptr()), LNK_OK);
        let event = Event {
            tick: 900,
            position: [5.0, 1.0, 2.0],
            strength: 0.9,
            creature_id: 7,
            kind: EventKind::Vocalisation as u8,
            reserved0: [0; 3],
        };
        assert_eq!((table.send_event)(connection, &raw const event), LNK_OK);
        let derez = Derez {
            tick: 901,
            creature_id: 3,
            reserved0: [0; 4],
        };
        assert_eq!((table.send_derez)(connection, &raw const derez), LNK_OK);
        let mut everything_left = 0u8;
        assert_eq!((table.flush)(connection, &raw mut everything_left), LNK_OK);
        assert_eq!(everything_left, 1);

        client_thread.join().expect("client thread");
        (table.close)(connection);
        (table.close_server)(server);
    }

    fn bodiless_rez(creature_id: u32) -> Rez {
        Rez {
            creature_id,
            max_forward_speed: 1.0,
            max_turn_rate: 1.5,
            max_vocalisation_strength: 1.0,
            max_contact_count: 4,
            vertex_count: 0,
            triangle_count: 0,
            material_count: 0,
        }
    }

    #[test]
    fn the_world_fingerprint_is_answered_through_the_table_and_a_null_is_nothing() {
        let table = vtable();
        let definition = WorldDefinition {
            floor_cells: 64,
            floor_cell_size: 2.0,
            floor_height: 0.0,
            relief_amplitude: 5.0,
            relief_wavelength: 46.0,
            relief_octaves: 3,
            relief_terraces: 6,
            relief_seed: 42,
            dt_seconds: 0.031_25,
            body_half_height: 0.5,
        };
        assert_eq!((table.world_fingerprint)(&raw const definition), world_fingerprint(&definition));
        assert_ne!((table.world_fingerprint)(&raw const definition), 0);
        assert_eq!((table.world_fingerprint)(std::ptr::null()), 0);
    }

    #[test]
    fn a_lying_rez_count_is_refused_before_a_single_row_is_read() {
        let table = vtable();
        let mut status: LnkStatus = -1;
        let server = (table.listen)(0, WORLD, &raw mut status, std::ptr::null_mut(), 0);
        let port = (table.server_port)(server);
        let client_thread = std::thread::spawn(move || {
            let table = vtable();
            let address = CString::new(format!("127.0.0.1:{port}")).expect("address");
            let mut welcome = unsafe { std::mem::zeroed::<Welcome>() };
            let mut status: LnkStatus = -1;
            let client = (table.connect)(
                address.as_ptr(),
                Role::CreatureHost as u8,
                WORLD,
                5_000,
                &raw mut welcome,
                &raw mut status,
                std::ptr::null_mut(),
                0,
            );
            assert_eq!(status, LNK_OK);

            let one_vertex = [RezVertex { position: [0.0; 3] }];
            for (vertices, triangles, materials) in [(REZ_MAX_VERTICES + 1, 0, 0), (0, REZ_MAX_TRIANGLES + 1, 0), (0, 0, REZ_MAX_MATERIALS + 1)] {
                let mut rez = bodiless_rez(1);
                rez.vertex_count = vertices;
                rez.triangle_count = triangles;
                rez.material_count = materials;
                assert_eq!(
                    (table.send_rez)(client, &raw const rez, one_vertex.as_ptr(), std::ptr::null(), std::ptr::null()),
                    LNK_BAD_ARGUMENT,
                    "a count over the cap is refused before a row is read"
                );
            }
            let mut rez = bodiless_rez(1);
            rez.vertex_count = 1;
            assert_eq!(
                (table.send_rez)(client, &raw const rez, std::ptr::null(), std::ptr::null(), std::ptr::null()),
                LNK_BAD_ARGUMENT,
                "a count with no rows behind it is refused before any read"
            );
            // An index past the vertices is the codec's refusal, surfaced as the caller's bad argument.
            let mut rez = bodiless_rez(1);
            rez.vertex_count = 1;
            rez.triangle_count = 1;
            rez.material_count = 1;
            let lying_triangle = [RezTriangle {
                vertices: [0, 0, 1],
                material: 0,
            }];
            let material = [RezMaterial {
                colour: [1.0; 3],
                index_of_refraction: 1.0,
                emission: [0.0; 3],
                transmission: 0.0,
            }];
            assert_eq!(
                (table.send_rez)(client, &raw const rez, one_vertex.as_ptr(), lying_triangle.as_ptr(), material.as_ptr()),
                LNK_BAD_ARGUMENT
            );
            assert_eq!(
                (table.send_rez)(client, std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null()),
                LNK_BAD_ARGUMENT
            );
            (table.close)(client);
        });

        let mut hello = unsafe { std::mem::zeroed::<Hello>() };
        let connection = loop {
            let knock = (table.accept)(server, 5_000, &raw mut hello, &raw mut status, std::ptr::null_mut(), 0);
            if !knock.is_null() {
                break knock;
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        let welcome = Welcome {
            current_tick: 1,
            nominal_dt_seconds: 0.031_25,
            client_id: 1,
            world_fingerprint: WORLD,
        };
        assert_eq!((table.send_welcome)(connection, &raw const welcome), LNK_OK);
        let mut everything_left = 0u8;
        assert_eq!((table.flush)(connection, &raw mut everything_left), LNK_OK);
        client_thread.join().expect("client thread");
        (table.close)(connection);
        (table.close_server)(server);
    }

    #[test]
    fn a_body_travels_the_table_both_ways_and_its_rows_outlive_the_poll() {
        let table = vtable();
        let mut status: LnkStatus = -1;
        let server = (table.listen)(0, WORLD, &raw mut status, std::ptr::null_mut(), 0);
        assert_eq!(status, LNK_OK);
        let port = (table.server_port)(server);

        let vertices = [
            RezVertex { position: [0.0, 0.0, 0.0] },
            RezVertex { position: [1.0, 0.0, 0.0] },
            RezVertex { position: [0.0, 1.0, 0.0] },
            RezVertex { position: [0.0, 0.0, 1.0] },
        ];
        let triangles = [
            RezTriangle {
                vertices: [0, 1, 2],
                material: 0,
            },
            RezTriangle {
                vertices: [0, 2, 3],
                material: 1,
            },
        ];
        let materials = [
            RezMaterial {
                colour: [0.9, 0.1, 0.1],
                index_of_refraction: 1.5,
                emission: [0.0; 3],
                transmission: 0.0,
            },
            RezMaterial {
                colour: [0.1, 0.9, 0.1],
                index_of_refraction: 1.0,
                emission: [0.0, 2.0, 0.0],
                transmission: 0.5,
            },
        ];
        let mut rez = bodiless_rez(7);
        rez.vertex_count = 4;
        rez.triangle_count = 2;
        rez.material_count = 2;

        // The host rezzes its body; the server hears it, then rezzes it back (the relay every
        // other citizen will receive), and the host reads its own body off the view.
        let client_thread = std::thread::spawn(move || {
            let table = vtable();
            let address = CString::new(format!("127.0.0.1:{port}")).expect("address");
            let mut welcome = unsafe { std::mem::zeroed::<Welcome>() };
            let mut status: LnkStatus = -1;
            let client = (table.connect)(
                address.as_ptr(),
                Role::CreatureHost as u8,
                WORLD,
                5_000,
                &raw mut welcome,
                &raw mut status,
                std::ptr::null_mut(),
                0,
            );
            assert_eq!(status, LNK_OK);
            assert_eq!(welcome.world_fingerprint, WORLD);
            assert_eq!(
                (table.send_rez)(client, &raw const rez, vertices.as_ptr(), triangles.as_ptr(), materials.as_ptr()),
                LNK_OK
            );
            let mut everything_left = 0u8;
            assert_eq!((table.flush)(client, &raw mut everything_left), LNK_OK);

            let mut view = unsafe { std::mem::zeroed::<MessageView>() };
            let deadline = std::time::Instant::now() + PATIENCE;
            loop {
                match (table.poll)(client, &raw mut view) {
                    LNK_NOTHING_YET => {
                        assert!(std::time::Instant::now() < deadline, "the body never came back");
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    LNK_OK => break,
                    other => panic!("poll answered {other}"),
                }
            }
            assert_eq!(view.message_type, MessageType::Rez as u8);
            // SAFETY: the type byte names the union member; the rows stay valid until the next
            // poll or close, per the header's contract.
            let echoed = unsafe { view.payload.rez };
            assert_eq!(echoed.rez.creature_id, 7);
            assert_eq!(echoed.rez.vertex_count, 4);
            let (third_vertex, second_triangle, second_material) = unsafe { (*echoed.vertices.add(2), *echoed.triangles.add(1), *echoed.materials.add(1)) };
            assert_eq!(third_vertex.position, [0.0, 1.0, 0.0]);
            assert_eq!(second_triangle.vertices, [0, 2, 3]);
            assert_eq!(second_triangle.material, 1);
            assert_eq!(second_material.emission, [0.0, 2.0, 0.0]);
            (table.close)(client);
        });

        let mut hello = unsafe { std::mem::zeroed::<Hello>() };
        let connection = loop {
            let knock = (table.accept)(server, 5_000, &raw mut hello, &raw mut status, std::ptr::null_mut(), 0);
            if !knock.is_null() {
                break knock;
            }
            assert_eq!(status, LNK_NOTHING_YET);
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(hello.world_fingerprint, WORLD);
        let welcome = Welcome {
            current_tick: 1,
            nominal_dt_seconds: 0.031_25,
            client_id: 1,
            world_fingerprint: WORLD,
        };
        assert_eq!((table.send_welcome)(connection, &raw const welcome), LNK_OK);
        let mut everything_left = 0u8;
        assert_eq!((table.flush)(connection, &raw mut everything_left), LNK_OK);

        let mut view = unsafe { std::mem::zeroed::<MessageView>() };
        let deadline = std::time::Instant::now() + PATIENCE;
        loop {
            match (table.poll)(connection, &raw mut view) {
                LNK_NOTHING_YET => {
                    assert!(std::time::Instant::now() < deadline, "the body never arrived");
                    std::thread::sleep(Duration::from_millis(1));
                }
                LNK_OK => break,
                other => panic!("poll answered {other}"),
            }
        }
        assert_eq!(view.message_type, MessageType::Rez as u8);
        // SAFETY: the type byte names the union member.
        let heard = unsafe { view.payload.rez };
        assert_eq!(heard.rez.triangle_count, 2);
        // Relay it straight back off the borrowed view: the rows are the library's own copies.
        assert_eq!(
            (table.send_rez)(connection, &raw const heard.rez, heard.vertices, heard.triangles, heard.materials),
            LNK_OK
        );
        assert_eq!((table.flush)(connection, &raw mut everything_left), LNK_OK);

        client_thread.join().expect("client thread");
        (table.close)(connection);
        (table.close_server)(server);
    }

    #[test]
    fn the_owners_letter_travels_the_table_one_way_and_its_contacts_outlive_the_poll() {
        let table = vtable();
        let mut status: LnkStatus = -1;
        let server = (table.listen)(0, WORLD, &raw mut status, std::ptr::null_mut(), 0);
        let port = (table.server_port)(server);

        let contacts = [
            Contact {
                position: [1.0, 0.0, 5.0],
                impulse: [0.0, 0.3, 0.0],
            },
            Contact {
                position: [1.1, 0.0, 5.0],
                impulse: [0.0, 0.4, 0.0],
            },
        ];
        let letter = Proprioception {
            tick: 9,
            creature_id: 7,
            grounded: 1,
            reserved0: [0; 3],
            specific_force: [0.0, 9.81, 0.0],
            contact_count: 2,
        };

        let client_thread = std::thread::spawn(move || {
            let table = vtable();
            let address = CString::new(format!("127.0.0.1:{port}")).expect("address");
            let mut welcome = unsafe { std::mem::zeroed::<Welcome>() };
            let mut status: LnkStatus = -1;
            let client = (table.connect)(
                address.as_ptr(),
                Role::CreatureHost as u8,
                WORLD,
                5_000,
                &raw mut welcome,
                &raw mut status,
                std::ptr::null_mut(),
                0,
            );
            assert_eq!(status, LNK_OK);
            // A client never sends the letter: refused at the sending half.
            assert_eq!((table.send_proprioception)(client, &raw const letter, contacts.as_ptr()), LNK_BAD_ARGUMENT);

            let mut view = unsafe { std::mem::zeroed::<MessageView>() };
            let deadline = std::time::Instant::now() + PATIENCE;
            loop {
                match (table.poll)(client, &raw mut view) {
                    LNK_NOTHING_YET => {
                        assert!(std::time::Instant::now() < deadline, "the letter never arrived");
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    LNK_OK => break,
                    other => panic!("poll answered {other}"),
                }
            }
            assert_eq!(view.message_type, MessageType::Proprioception as u8);
            // SAFETY: the type byte names the union member; the contacts stay valid until the
            // next poll or close, per the header's contract.
            let heard = unsafe { view.payload.proprioception };
            assert_eq!(heard.proprioception, letter);
            assert_eq!(unsafe { *heard.contacts.add(1) }, contacts[1]);
            (table.close)(client);
        });

        let mut hello = unsafe { std::mem::zeroed::<Hello>() };
        let connection = loop {
            let knock = (table.accept)(server, 5_000, &raw mut hello, &raw mut status, std::ptr::null_mut(), 0);
            if !knock.is_null() {
                break knock;
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        let welcome = Welcome {
            current_tick: 9,
            nominal_dt_seconds: 0.031_25,
            client_id: 1,
            world_fingerprint: WORLD,
        };
        assert_eq!((table.send_welcome)(connection, &raw const welcome), LNK_OK);
        // The caps are judged before a row is read, here too.
        let mut lying = letter;
        lying.contact_count = CONTACTS_MAX + 1;
        assert_eq!((table.send_proprioception)(connection, &raw const lying, contacts.as_ptr()), LNK_BAD_ARGUMENT);
        lying.contact_count = 1;
        assert_eq!((table.send_proprioception)(connection, &raw const lying, std::ptr::null()), LNK_BAD_ARGUMENT);
        assert_eq!((table.send_proprioception)(connection, &raw const letter, contacts.as_ptr()), LNK_OK);
        let mut everything_left = 0u8;
        assert_eq!((table.flush)(connection, &raw mut everything_left), LNK_OK);

        client_thread.join().expect("client thread");
        (table.close)(connection);
        (table.close_server)(server);
    }

    #[test]
    fn a_different_world_is_refused_through_the_table_in_both_directions() {
        let table = vtable();
        let mut status: LnkStatus = -1;
        let server = (table.listen)(0, WORLD, &raw mut status, std::ptr::null_mut(), 0);
        let port = (table.server_port)(server);

        let client_thread = std::thread::spawn(move || {
            let table = vtable();
            let address = CString::new(format!("127.0.0.1:{port}")).expect("address");
            let mut welcome = unsafe { std::mem::zeroed::<Welcome>() };
            let mut status: LnkStatus = -1;
            let mut detail = [0i8; 256];
            let client = (table.connect)(
                address.as_ptr(),
                Role::CreatureHost as u8,
                WORLD + 1,
                5_000,
                &raw mut welcome,
                &raw mut status,
                detail.as_mut_ptr().cast::<c_char>(),
                detail.len() as u32,
            );
            assert!(client.is_null());
            assert_eq!(status, LNK_REFUSED);
            unsafe { CStr::from_ptr(detail.as_ptr().cast::<c_char>()) }.to_string_lossy().to_string()
        });

        let mut hello = unsafe { std::mem::zeroed::<Hello>() };
        let mut detail = [0i8; 256];
        let deadline = std::time::Instant::now() + PATIENCE;
        loop {
            let knock = (table.accept)(
                server,
                5_000,
                &raw mut hello,
                &raw mut status,
                detail.as_mut_ptr().cast::<c_char>(),
                detail.len() as u32,
            );
            assert!(knock.is_null(), "a citizen of another world must not be accepted");
            if status != LNK_NOTHING_YET {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "nobody knocked");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(status, LNK_REFUSED);
        let server_words = unsafe { CStr::from_ptr(detail.as_ptr().cast::<c_char>()) }.to_string_lossy().to_string();
        assert!(server_words.contains("different world"), "the server's log line names the cause, got: {server_words}");
        let client_words = client_thread.join().expect("client thread");
        assert!(
            client_words.contains(&format!("{WORLD:016X}")),
            "the client hears which world it was refused by, got: {client_words}"
        );
        (table.close_server)(server);
    }

    #[test]
    fn a_disk_written_through_the_table_replays_through_it_and_refuses_what_a_server_would() {
        let table = vtable();
        let mut path = std::env::temp_dir();
        path.push(format!("link-abi-disk-{}.disk", std::process::id()));
        let c_path = CString::new(path.to_string_lossy().to_string()).expect("path");

        let mut status: LnkStatus = -1;
        let disk = (table.record_open)(c_path.as_ptr(), WORLD, 500, 0.031_25, 1_700_000_000, &raw mut status, std::ptr::null_mut(), 0);
        assert_eq!(status, LNK_OK);
        assert!(!disk.is_null());

        // A recording is a server-held end: the world's messages stage, poll hears nothing.
        let rows = [CreatureState {
            creature_id: 7,
            position: [1.0, 2.0, 3.0],
            yaw: 0.5,
            velocity: [0.0; 3],
            yaw_rate: 0.0,
            vocalisation: 0.0,
        }];
        let header = TickStateHeader {
            tick: 501,
            creature_count: 1,
            reserved0: [0; 4],
        };
        assert_eq!((table.send_tick_state)(disk, &raw const header, rows.as_ptr()), LNK_OK);
        let letter = Proprioception {
            tick: 501,
            creature_id: 7,
            grounded: 1,
            reserved0: [0; 3],
            specific_force: [0.0, 9.81, 0.0],
            contact_count: 0,
        };
        assert_eq!((table.send_proprioception)(disk, &raw const letter, std::ptr::null()), LNK_OK);
        let mut view = unsafe { std::mem::zeroed::<MessageView>() };
        assert_eq!((table.poll)(disk, &raw mut view), LNK_NOTHING_YET);
        let mut everything_left = 0u8;
        assert_eq!((table.flush)(disk, &raw mut everything_left), LNK_OK);
        assert_eq!(everything_left, 1, "a file never says later");
        (table.close)(disk);

        // Another world is refused in words; the right world opens with a WELCOME-shaped start.
        let mut welcome = unsafe { std::mem::zeroed::<Welcome>() };
        let mut detail = [0i8; 256];
        let wrong = (table.replay_open)(
            c_path.as_ptr(),
            WORLD + 1,
            &raw mut welcome,
            &raw mut status,
            detail.as_mut_ptr().cast::<c_char>(),
            detail.len() as u32,
        );
        assert!(wrong.is_null());
        assert_eq!(status, LNK_REFUSED);
        let words = unsafe { CStr::from_ptr(detail.as_ptr().cast::<c_char>()) }.to_string_lossy().to_string();
        assert!(words.contains("different world"), "{words}");

        let replay = (table.replay_open)(c_path.as_ptr(), WORLD, &raw mut welcome, &raw mut status, std::ptr::null_mut(), 0);
        assert_eq!(status, LNK_OK);
        assert_eq!(welcome.current_tick, 500);
        assert_eq!(welcome.world_fingerprint, WORLD);
        assert_eq!(welcome.client_id, 0);

        // A replay has nobody to talk to.
        assert_eq!((table.send_ping)(replay, 1), LNK_BAD_ARGUMENT);
        assert_eq!((table.send_tick_state)(replay, &raw const header, rows.as_ptr()), LNK_BAD_ARGUMENT);

        // What was said, in order, then the farewell, then the end.
        assert_eq!((table.poll)(replay, &raw mut view), LNK_OK);
        assert_eq!(view.message_type, MessageType::TickState as u8);
        // SAFETY: the type byte names the union member.
        assert_eq!(unsafe { (*view.payload.tick_state.states).creature_id }, 7);
        assert_eq!((table.poll)(replay, &raw mut view), LNK_OK);
        assert_eq!(view.message_type, MessageType::Proprioception as u8);
        assert_eq!((table.poll)(replay, &raw mut view), LNK_OK);
        assert_eq!(view.message_type, MessageType::Bye as u8);
        assert_eq!((table.poll)(replay, &raw mut view), LNK_PEER_CLOSED, "the end of the Disk is the world closing");
        (table.close)(replay);

        // Nulls are refused, never dereferenced.
        assert!((table.record_open)(std::ptr::null(), WORLD, 0, 0.0, 0, &raw mut status, std::ptr::null_mut(), 0).is_null());
        assert_eq!(status, LNK_BAD_ARGUMENT);
        assert!((table.replay_open)(c_path.as_ptr(), WORLD, std::ptr::null_mut(), &raw mut status, std::ptr::null_mut(), 0).is_null());
        assert_eq!(status, LNK_BAD_ARGUMENT);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_lying_tick_state_count_is_refused_before_a_single_row_is_read() {
        let table = vtable();
        let one_row = [CreatureState {
            creature_id: 0,
            position: [0.0; 3],
            yaw: 0.0,
            velocity: [0.0; 3],
            yaw_rate: 0.0,
            vocalisation: 0.0,
        }];
        let over_cap = TickStateHeader {
            tick: 1,
            creature_count: TICK_STATE_MAX_CREATURES + 1,
            reserved0: [0; 4],
        };
        assert_eq!(
            (table.send_tick_state)(std::ptr::null_mut(), &raw const over_cap, one_row.as_ptr()),
            LNK_BAD_ARGUMENT,
            "a count over the cap is refused before rows or connection are even looked at"
        );
        let one_claimed = TickStateHeader {
            tick: 1,
            creature_count: 1,
            reserved0: [0; 4],
        };
        assert_eq!(
            (table.send_tick_state)(std::ptr::null_mut(), &raw const one_claimed, std::ptr::null()),
            LNK_BAD_ARGUMENT,
            "a count with no rows behind it is refused before any read"
        );
    }

    #[test]
    fn the_server_half_refuses_nulls_too() {
        let table = vtable();
        assert!(
            (table.listen)(0, WORLD, std::ptr::null_mut(), std::ptr::null_mut(), 0).is_null(),
            "no status pointer, no server"
        );
        assert_eq!((table.server_port)(std::ptr::null_mut()), 0);
        let mut hello = unsafe { std::mem::zeroed::<Hello>() };
        let mut status: LnkStatus = -1;
        assert!((table.accept)(std::ptr::null_mut(), 0, &raw mut hello, &raw mut status, std::ptr::null_mut(), 0).is_null());
        assert_eq!(status, LNK_BAD_ARGUMENT);
        assert_eq!((table.send_welcome)(std::ptr::null_mut(), std::ptr::null()), LNK_BAD_ARGUMENT);
        assert_eq!((table.send_event)(std::ptr::null_mut(), std::ptr::null()), LNK_BAD_ARGUMENT);
        assert_eq!((table.send_derez)(std::ptr::null_mut(), std::ptr::null()), LNK_BAD_ARGUMENT);
        (table.close_server)(std::ptr::null_mut());
    }

    /// The cross-language twin check without a C compiler: the header's #define lines are
    /// parsed out of the header itself, which is compiled into this test, and each one must
    /// equal the Rust constant it mirrors. A drift refuses here, red, immediately.
    #[test]
    fn the_header_and_the_rust_constants_are_the_same_constants() {
        let header = include_str!("../include/lnk/lnk_client.h");
        let pins: &[(&str, i64)] = &[
            ("LNK_CLIENT_ABI_VERSION", i64::from(LNK_CLIENT_ABI_VERSION)),
            ("LNK_OK", i64::from(LNK_OK)),
            ("LNK_NOTHING_YET", i64::from(LNK_NOTHING_YET)),
            ("LNK_REFUSED", i64::from(LNK_REFUSED)),
            ("LNK_HANDSHAKE_TIMED_OUT", i64::from(LNK_HANDSHAKE_TIMED_OUT)),
            ("LNK_PEER_CLOSED", i64::from(LNK_PEER_CLOSED)),
            ("LNK_FRAME_REFUSED", i64::from(LNK_FRAME_REFUSED)),
            ("LNK_GARBLED", i64::from(LNK_GARBLED)),
            ("LNK_IO", i64::from(LNK_IO)),
            ("LNK_BAD_ARGUMENT", i64::from(LNK_BAD_ARGUMENT)),
            ("LNK_PANIC", i64::from(LNK_PANIC)),
        ];
        for (name, value) in pins {
            let defined = header
                .lines()
                .find_map(|line| {
                    let rest = line.strip_prefix("#define ")?.strip_prefix(name)?.trim();
                    (!rest.is_empty() && !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
                        .then_some(rest)
                        .or(Some(rest))
                        .filter(|_| line.split_whitespace().nth(1) == Some(name))
                })
                .unwrap_or_else(|| panic!("{name} is not defined in lnk_client.h"));
            let digits: String = defined.chars().take_while(|c| c.is_ascii_digit()).collect();
            assert_eq!(
                digits.parse::<i64>().expect("a numeric define"),
                *value,
                "{name} drifted between the header and the Rust side"
            );
        }
    }
}
