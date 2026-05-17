//! Packet server for threaded decoding and encoding of packets. This module is generic
//! and could be used for any TCP and packet-based protocol. It is specialized in the
//! [`proto`](crate::proto) crate.

use std::io::{self, Read, Write, Cursor};
use std::net::{SocketAddr, Shutdown};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
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

            let token = self.next_token;
            self.next_token = Token(token.0.checked_add(1).expect("out of client token"));
            self.poll.registry().register(&mut stream, token, Interest::READABLE | Interest::WRITABLE)?;

            let shared = Arc::new(SharedClient {
                stream: RwLock::new(stream),
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
            self.handle_client_close(token, Some(new_io_abort_error("client side closed")))
        } else if event.is_readable() {
            match self.handle_client_read(token) {
                Err(e) => self.handle_client_close(token, Some(e)),
                Ok(run) => run
            }
        } else {
            true
        }
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
        
        eprintln!("[DEBUG] Trying to read packet, buffer size: {}", buf.len());
        
        let packet = match I::read(&mut cursor) {
    Ok(packet) => {
        eprintln!("[DEBUG] Packet raw data (first 16 bytes): {:02x?}", &buf[..16.min(buf.len())]);
        packet
    },
    Err(e) => {
        eprintln!("[DEBUG] Failed to read packet: {}", e);
        
        let mut decoder = ZlibDecoder::new(&buf[..]);
        let mut decompressed = Vec::new();
        if decoder.read_to_end(&mut decompressed).is_ok() {
            eprintln!("[DEBUG] Decompressed {} bytes", decompressed.len());
            eprintln!("[DEBUG] Decompressed raw data: {:02x?}", &decompressed[..16.min(decompressed.len())]);
            let mut new_cursor = Cursor::new(decompressed);
            match I::read(&mut new_cursor) {
                Ok(packet) => packet,
                Err(e2) => {
                    eprintln!("[DEBUG] Still failed after decompression: {}", e2);
                    return Err(e);
                }
            }
        } else {
            eprintln!("[DEBUG] Decompression failed");
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
        let stream = client.stream.read().expect("poisoned");
        let _ = packet.write(&mut &*stream);
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