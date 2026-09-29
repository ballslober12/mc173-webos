//! Packet server for threaded decoding and encoding of packets. This module is generic
//! and could be used for any TCP and packet-based protocol. It is specialized in the
//! [`proto`](crate::proto) crate.

use std::io::{self, Read, Write, Cursor};
use std::net::{SocketAddr, Shutdown};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use std::thread;
use std::fmt;

use crossbeam_channel::{bounded, Sender, Receiver, TryRecvError};

use mio::{Poll, Events, Interest, Token};
use mio::net::{TcpListener, TcpStream};
use mio::event::Event;

use flate2::read::ZlibDecoder;

/// A server-bound packet (received and processed by the server).
pub trait InPacket: Sized {
    /// Read the packet from the writer.
    fn read(read: &mut impl Read) -> io::Result<Self>;
}

/// A client-bound packet (received and processed by the client).
pub trait OutPacket {
    /// Write the packet to the given writer.
    fn write(&self, write: &mut impl Write) -> io::Result<()>;
}

/// A packet server backed by a background thread that do all the hard processing. This
/// network handle can be cloned as need, and every handle is able to both send and
/// receive packets.
/// 
/// To kill the server, every handle of it should be dropped.
#[derive(Debug, Clone)]
pub struct Network<I, O> {
    /// This channels allows sending commands to the thread.
    commands_sender: Sender<ThreadCommand<O>>,
    /// This channels allows received events from the thread.
    events_receiver: Receiver<ThreadEvent<I>>,
}

impl<I, O> Network<I, O>
where
    I: InPacket + Send + 'static,
    O: OutPacket + Send + 'static,
{

    pub fn bind(addr: SocketAddr) -> io::Result<Self> {

        let poll = Poll::new()?;
        let mut listener = TcpListener::bind(addr)?;
        poll.registry().register(&mut listener, LISTENER_TOKEN, Interest::READABLE)?;

        let (
            commands_sender,
            commands_receiver
        ) = bounded(1000);

        let (
            events_sender,
            events_receiver
        ) = bounded(1000);

        // The poll thread.
        let poll_commands_sender = commands_sender.clone();
        
        thread::Builder::new()
            .name("Packet Poll Thread".to_string())
            .spawn(move || {
                PollThread::<I, O> {
                    commands_sender: poll_commands_sender,
                    events_sender,
                    listener,
                    poll,
                    next_token: CLIENT_FIRST_TOKEN,
                    clients: HashMap::new(),
                }.run();
            }).unwrap();

        // The command thread.
        thread::Builder::new()
            .name("Packet Command Thread".to_string())
            .spawn(move || {
                CommandThread::<O> {
                    commands_receiver,
                    clients: HashMap::new(),
                }.run();
            }).unwrap();

        Ok(Self {
            commands_sender,
            events_receiver,
        })

    }

    /// Poll events from this packet server. If an I/O error is returned, the error is
    /// critical and the 
    pub fn poll(&self) -> io::Result<Option<NetworkEvent<I>>> {
        loop {
            return Ok(Some(match self.events_receiver.try_recv() {
                Ok(ThreadEvent::ChannelCheck) => continue,
                Ok(ThreadEvent::Accept { token }) => NetworkEvent::Accept {
                    client: NetworkClient(token)
                },
                Ok(ThreadEvent::Lost { token, error }) => NetworkEvent::Lost {
                    client: NetworkClient(token),
                    error,
                },
                Ok(ThreadEvent::Packet { token, packet }) => NetworkEvent::Packet {
                    client: NetworkClient(token), 
                    packet,
                },
                Ok(ThreadEvent::Error { error }) => return Err(error), 
                Err(TryRecvError::Empty) => return Ok(None),
                Err(TryRecvError::Disconnected) => 
                    return Err(new_io_abort_error("previous error made this server unusable")),
            }));
        }
    }

    pub fn send(&self, client: NetworkClient, packet: O) {
        self.commands_sender.try_send(ThreadCommand::SingleClientPacket { 
            token: client.0, 
            packet
        }).expect("commands channel is full");
    }

    pub fn disconnect(&self, client: NetworkClient) {
        self.commands_sender.try_send(ThreadCommand::DisconnectClient {
            token: client.0
        }).expect("commands channel is full");
    }

}

/// A handle to a client produced by a packet server. This handle can be used with a
/// server to send packets to a client.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NetworkClient(Token);

impl fmt::Debug for NetworkClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("NetworkClient").field(&self.0.0).finish()
    }
}

impl NetworkClient {
    #[inline]
    pub fn id(self) -> u64 {
        self.0.0 as u64
    }
}

#[derive(Debug)]
pub enum NetworkEvent<I> {
    Accept {
        client: NetworkClient,
    },
    Lost {
        client: NetworkClient,
        error: Option<io::Error>,
    },
    Packet {
        client: NetworkClient,
        packet: I,
    },
}

const LISTENER_TOKEN: Token = Token(0);
const CLIENT_FIRST_TOKEN: Token = Token(1);
const BUF_SIZE: usize = 65536; // Увеличил буфер

struct SharedClient {
    stream: RwLock<TcpStream>,
    /// Bytes queued for this client that a previous non-blocking write couldn't
    /// accept yet. Must be drained (via `flush`) before any new bytes are sent, or
    /// packets would arrive at the client out of order / torn mid-packet.
    out_buf: Mutex<Vec<u8>>,
}

impl SharedClient {

    /// Write as much of `data` as the socket accepts right now without blocking.
    /// Returns the number of bytes actually written, which may be less than
    /// `data.len()` if the socket's send buffer is full.
    fn write_partial(write: &mut impl Write, data: &[u8]) -> io::Result<usize> {
        let mut written = 0;
        while written < data.len() {
            match write.write(&data[written..]) {
                Ok(0) => return Err(io::Error::new(io::ErrorKind::WriteZero, "write returned 0")),
                Ok(n) => written += n,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(written)
    }

    /// Queue a fully-encoded packet's bytes for sending. If nothing is already
    /// pending, tries to write directly; on a non-blocking socket the write may only
    /// go through partially (or not at all), in which case the remainder is buffered
    /// instead of being silently dropped, so `flush` can finish it later. This never
    /// leaves a packet torn mid-stream, which corrupted zlib-compressed chunk data
    /// for the client (as "bad compressed data format") whenever the send buffer
    /// filled up under load.
    fn queue_write(&self, data: &[u8]) -> io::Result<()> {
        let mut out_buf = self.out_buf.lock().expect("poisoned");
        if out_buf.is_empty() {
            let stream = self.stream.read().expect("poisoned");
            let written = Self::write_partial(&mut &*stream, data)?;
            if written < data.len() {
                out_buf.extend_from_slice(&data[written..]);
            }
        } else {
            // Bytes from an earlier packet are still waiting to go out; appending
            // keeps packet order intact instead of racing ahead of them.
            out_buf.extend_from_slice(data);
        }
        Ok(())
    }

    /// Try to send any bytes left over from a previous `queue_write`. Call this when
    /// the socket reports writable.
    fn flush(&self) -> io::Result<()> {
        let mut out_buf = self.out_buf.lock().expect("poisoned");
        if out_buf.is_empty() {
            return Ok(());
        }
        let stream = self.stream.read().expect("poisoned");
        let written = Self::write_partial(&mut &*stream, &out_buf)?;
        out_buf.drain(..written);
        Ok(())
    }

}

struct PollThread<I, O> {
    commands_sender: Sender<ThreadCommand<O>>,
    events_sender: Sender<ThreadEvent<I>>,
    listener: TcpListener,
    poll: Poll,
    next_token: Token,
    clients: HashMap<Token, PollClient>,
}

struct PollClient {
    shared: Arc<SharedClient>,
    buf: Box<[u8; BUF_SIZE]>,
    buf_cursor: usize,
}

impl<I: InPacket, O: OutPacket> PollThread<I, O> {

    fn run(mut self) {
        let mut events = Events::with_capacity(100);
        while self.events_sender.send(ThreadEvent::ChannelCheck).is_ok() {
            if let Err(e) = self.poll(&mut events) {
                let _ = self.events_sender.send(ThreadEvent::Error { error: e });
                return;
            }
        }
    }

    fn poll(&mut self, events: &mut Events) -> io::Result<bool> {
        self.poll.poll(events, Some(Duration::from_secs(1)))?;
        for event in events.iter() {
            let run = match event.token() {
                LISTENER_TOKEN => self.handle_listener()?,
                _ => self.handle_client(event),
            };
            if !run {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn handle_listener(&mut self) -> io::Result<bool> {
        loop {
            let mut stream = match self.listener.accept() {
                Ok((stream, _addr)) => stream,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(true),
                Err(e) => return Err(e),
            };

            // Disable Nagle's algorithm: without this, small packets (which is most of
            // what this protocol sends - position updates, block changes, keep-alives)
            // can sit buffered for tens of milliseconds waiting to be coalesced with
            // more data before being sent, which is the single biggest contributor to
            // perceived "ping"/input lag on a TCP game server. This trades a few extra
            // small TCP segments for much lower latency, which is the right trade-off
            // here.
            if let Err(e) = stream.set_nodelay(true) {
                let _ = e; // Not fatal: keep going even if the platform refuses this.
            }

            let token = self.next_token;
            self.next_token = Token(token.0.checked_add(1).expect("out of client token"));
            self.poll.registry().register(&mut stream, token, Interest::READABLE | Interest::WRITABLE)?;

            let shared = Arc::new(SharedClient {
                stream: RwLock::new(stream),
                out_buf: Mutex::new(Vec::new()),
            });

            self.commands_sender.send(ThreadCommand::NewClient { token, shared: Arc::clone(&shared) })
                .expect("commands channel should not be disconnected");

            if self.events_sender.send(ThreadEvent::Accept { token }).is_err() {
                return Ok(false);
            }

            self.clients.insert(token, PollClient {
                shared, 
                buf: Box::new([0; BUF_SIZE]),
                buf_cursor: 0
            });
        }
    }

    fn handle_client(&mut self, event: &Event) -> bool {
        let token = event.token();
        if event.is_read_closed() || event.is_write_closed() {
            return self.handle_client_close(token, Some(new_io_abort_error("client side closed")));
        }

        // Drain any bytes a previous non-blocking send couldn't fit in the socket
        // buffer. Must happen before reading so a client isn't dropped mid-flush.
        if event.is_writable() {
            if let Some(client) = self.clients.get(&token) {
                if let Err(e) = client.shared.flush() {
                    if e.kind() != io::ErrorKind::WouldBlock {
                        return self.handle_client_close(token, Some(e));
                    }
                }
            }
        }

        if event.is_readable() {
            return match self.handle_client_read(token) {
                Err(e) => self.handle_client_close(token, Some(e)),
                Ok(run) => run
            };
        }

        true
    }
fn handle_client_read(&mut self, token: Token) -> io::Result<bool> {
    let Some(client) = self.clients.get_mut(&token) else { return Ok(true) };
    let stream = client.shared.stream.read().expect("poisoned");
    let mut stream = &*stream;

    loop {
        match stream.read(&mut client.buf[client.buf_cursor..]) {
            Ok(0) => break,
            Ok(len) => client.buf_cursor += len,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }

    loop {
        let buf = &client.buf[..client.buf_cursor];
        if buf.len() == 0 {
            return Ok(true);
        }

        let mut cursor = Cursor::new(buf);

        let packet = match I::read(&mut cursor) {
            Ok(packet) => packet,
            Err(e) => {
                // Fallback: some clients occasionally send a zlib-wrapped packet body.
                // Try decompressing before giving up.
                let mut decoder = ZlibDecoder::new(&buf[..]);
                let mut decompressed = Vec::new();
                if decoder.read_to_end(&mut decompressed).is_ok() {
                    let mut new_cursor = Cursor::new(decompressed);
                    match I::read(&mut new_cursor) {
                        Ok(packet) => packet,
                        Err(_) => return Err(e),
                    }
                } else {
                    return Err(e);
                }
            }
        };

        if self.events_sender.send(ThreadEvent::Packet { token, packet }).is_err() {
            return Ok(false);
        }

        let read_length = cursor.position() as usize;
        drop(cursor);
        client.buf.copy_within(read_length..client.buf_cursor, 0);
        client.buf_cursor -= read_length;
    }
}

    fn handle_client_close(&mut self, token: Token, error: Option<io::Error>) -> bool {
        let Some(client) = self.clients.remove(&token) else { return true; };
        let mut stream = client.shared.stream.write().expect("poisoned");
        let _ = stream.shutdown(Shutdown::Both);
        let _ = self.poll.registry().deregister(&mut *stream);
        self.commands_sender.send(ThreadCommand::LostClient { token })
            .expect("commands channel should not be disconnected");
        self.events_sender.send(ThreadEvent::Lost { token, error }).is_ok()
    }
}

struct CommandThread<O> {
    commands_receiver: Receiver<ThreadCommand<O>>,
    clients: HashMap<Token, Arc<SharedClient>>,
}

impl<O: OutPacket> CommandThread<O> {
    fn run(mut self) {
        while let Ok(command) = self.commands_receiver.recv() {
            match command {
                ThreadCommand::NewClient { token, shared } => {
                    self.clients.insert(token, shared);
                }
                ThreadCommand::LostClient { token } => {
                    self.clients.remove(&token);
                }
                ThreadCommand::DisconnectClient { token } => {
                    self.handle_client_disconnect(token);
                }
                ThreadCommand::SingleClientPacket { token, packet } => {
                    self.handle_client_send(token, packet);
                }
            }
        }
    }

    fn handle_client_disconnect(&mut self, token: Token) {
        let Some(client) = self.clients.get(&token) else { return };
        let stream = client.stream.read().expect("poisoned");
        let _ = stream.shutdown(Shutdown::Both);
    }

    fn handle_client_send(&mut self, token: Token, packet: O) {
        let Some(client) = self.clients.get(&token) else { return };
        // Encode into memory first: queue_write needs a plain byte slice so it can
        // buffer the whole thing atomically if the non-blocking socket can't take it
        // all right now, rather than writing straight to the socket and losing
        // whatever didn't fit (which used to tear packets - e.g. truncating a
        // zlib-compressed chunk payload mid-stream and desyncing everything sent
        // after it).
        let mut data = Vec::new();
        if packet.write(&mut data).is_err() {
            return;
        }
        let _ = client.queue_write(&data);
    }
}

enum ThreadCommand<O> {
    NewClient { token: Token, shared: Arc<SharedClient> },
    LostClient { token: Token },
    DisconnectClient { token: Token },
    SingleClientPacket { token: Token, packet: O },
}

enum ThreadEvent<I> {
    ChannelCheck,
    Accept { token: Token },
    Lost { token: Token, error: Option<io::Error> },
    Packet { token: Token, packet: I },
    Error { error: io::Error },
}

fn new_io_abort_error(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::ConnectionAborted, message)
}