//! Ten independent Roha identities exchanging lab-only text over direct QUIC.
//! This intentionally is NOT the production WOW envelope or an offline messenger.
use std::{
    collections::{HashMap, HashSet},
    env,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    net::ToSocketAddrs,
    path::Path,
    sync::Arc,
    time::Duration,
};

use iroh::{
    Endpoint, EndpointAddr, EndpointId, RelayMode, SecretKey, TransportAddr,
    endpoint::{Connection, presets},
};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::Mutex,
    time::timeout,
};

type LabResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
const ALPN: &[u8] = b"akula/roha/three-net-mailbox/1";
const PORT: u16 = 9444;
const STATE: &str = "/state";
const SOCKET: &str = "/state/control.sock";
const MAX_FRAME: usize = 8192;

#[derive(Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
enum Command {
    Id,
    Count,
    Allow {
        key: String,
    },
    Send {
        host: String,
        key: String,
        id: String,
        text: String,
    },
    RelaySend { host: String, key: String, recipient: String, id: String, text: String },
    Forward { host: String, key: String, id: String },
    Pull { host: String, key: String },
}

#[derive(Serialize, Deserialize)]
struct Message {
    id: String,
    text: String,
}

#[derive(Serialize, Deserialize)]
struct Stored {
    from: String,
    id: String,
    text: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct Envelope {
    sender: String,
    recipient: String,
    id: String,
    text: String,
    signature: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Packet {
    Store { envelope: Envelope },
    Pull,
}


struct State {
    trusted: Mutex<HashSet<String>>,
    seen: Mutex<HashMap<(String, String), String>>,
    mailbox: Mutex<HashMap<String, Envelope>>,
}

fn lines(path: &str) -> LabResult<Vec<String>> {
    match File::open(path) {
        Ok(file) => Ok(BufReader::new(file)
            .lines()
            .collect::<io::Result<Vec<_>>>()?),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

fn write_durable(path: &str, value: &str) -> LabResult<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(value.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn identity() -> LabResult<SecretKey> {
    fs::create_dir_all(STATE)?;
    let path = Path::new(STATE).join("identity.key");
    if path.exists() {
        let existing: [u8; 32] = fs::read(&path)?
            .try_into()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid identity file"))?;
        return Ok(SecretKey::from_bytes(&existing));
    }
    let secret = SecretKey::generate();
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(&secret.to_bytes())?;
    file.sync_all()?;
    Ok(secret)
}

impl State {
    fn open() -> LabResult<Self> {
        let mut seen = HashMap::new();
        for line in lines("/state/inbox.jsonl")? {
            let record: Stored = serde_json::from_str(&line)?;
            let key = (record.from, record.id);
            if let Some(previous) = seen.insert(key, record.text.clone()) {
                if previous != record.text {
                    return Err(
                        io::Error::new(io::ErrorKind::InvalidData, "conflicting inbox").into(),
                    );
                }
            }
        }
        let mut mailbox = HashMap::new();
        for row in lines("/state/mailbox.jsonl")? {
            let env: Envelope = serde_json::from_str(&row)?;
            mailbox.insert(env.id.clone(), env);
        }
        Ok(Self {
            trusted: Mutex::new(lines("/state/trusted.txt")?.into_iter().collect()),
            seen: Mutex::new(seen),
            mailbox: Mutex::new(mailbox),
        })
    }

    async fn allow(&self, key: String) -> LabResult<String> {
        let parsed: EndpointId = key.parse()?;
        let key = parsed.to_string();
        let mut trusted = self.trusted.lock().await;
        if trusted.contains(&key) {
            return Ok("ALREADY_ALLOWED".into());
        }
        write_durable("/state/trusted.txt", &key)?;
        trusted.insert(key);
        Ok("ALLOWED".into())
    }

    async fn store(&self, env: Envelope) -> LabResult<String> {
        if env.id.is_empty() || env.id.len() > 80 || env.text.len() > 1024 {
            return Ok("REJECTED".into());
        }
        let sender: EndpointId = env.sender.parse()?;
        let _: EndpointId = env.recipient.parse()?;
        let signature: [u8; 64] = env.signature.as_slice().try_into()?;
        let tbs = serde_json::to_vec(&(&env.sender, &env.recipient, &env.id, &env.text))?;
        if sender.verify(&tbs, &iroh::Signature::from_bytes(&signature)).is_err() {
            return Ok("BAD_SIGNATURE".into());
        }
        let mut mailbox = self.mailbox.lock().await;
        if let Some(old) = mailbox.get(&env.id) {
            return if serde_json::to_vec(old)? == serde_json::to_vec(&env)? {
                Ok("DUPLICATE".into())
            } else {
                Ok("CONFLICT".into())
            };
        }
        if mailbox.len() >= 128 {
            return Ok("FULL".into());
        }
        // This first lab stores plaintext: NOT safe for personal messages.
        write_durable("/state/mailbox.jsonl", &serde_json::to_string(&env)?)?;
        mailbox.insert(env.id.clone(), env);
        Ok("STORED".into())
    }

    async fn persist(&self, from: String, msg: Message) -> LabResult<String> {
        if msg.id.is_empty()
            || msg.id.len() > 100
            || !msg
                .id
                .bytes()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == b'-' || ch == b'_')
            || msg.text.len() > 1024
        {
            return Ok("REJECTED".into());
        }
        let mut seen = self.seen.lock().await;
        let key = (from.clone(), msg.id.clone());
        match seen.get(&key) {
            Some(previous) if previous == &msg.text => return Ok("DUPLICATE".into()),
            Some(_) => return Ok("CONFLICT".into()),
            None => {}
        }
        let data = serde_json::to_string(&Stored {
            from,
            id: msg.id,
            text: msg.text.clone(),
        })?;
        write_durable("/state/inbox.jsonl", &data)?;
        seen.insert(key, msg.text);
        Ok("STORED".into())
    }
}

async fn incoming(connection: Connection, state: Arc<State>) -> LabResult<()> {
    let sender = connection.remote_id().to_string();
    println!("QUIC_INCOMING sender={sender}");
    let (mut tx, mut rx) = connection.accept_bi().await?;
    if !state.trusted.lock().await.contains(&sender) {
        tx.write_all(b"DENIED").await?;
        tx.finish()?;
        return Ok(());
    }
    let bytes = rx.read_to_end(MAX_FRAME).await?;
    let packet: Packet = serde_json::from_slice(&bytes)?;
    let reply = match packet {
        Packet::Store { envelope } => {
            // Only explicitly enrolled transport peers may deposit envelopes.
            let id = envelope.id.clone();
            let status = state.store(envelope).await?;
            println!("MAILBOX id={id} status={status}");
            format!("ACK:{id}:{status}")
        }
        Packet::Pull => {
            let mailbox = state.mailbox.lock().await;
            let mut mine: Vec<Envelope> = mailbox.values()
                .filter(|e| e.recipient == sender).cloned().collect();
            mine.sort_by(|a,b| a.id.cmp(&b.id));
            serde_json::to_string(&mine)?
        }
    };
    tx.write_all(reply.as_bytes()).await?;
    tx.finish()?;
    // Dropping the final QUIC connection immediately after finish may abort
    // the peer's read of the durable receipt; wait for the client to close.
    let _ = timeout(Duration::from_secs(5), connection.closed()).await;
    Ok(())
}

async fn rpc(endpoint: &Endpoint, host: String, key: String, packet: Packet) -> LabResult<String> {
    if host.len() > 24 || !host.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-') {
        return Err("invalid hostname".into());
    }
    let peer: EndpointId = key.parse()?;
    let ip = (host.as_str(), PORT).to_socket_addrs()?.next().ok_or("missing peer")?;
    let target = EndpointAddr::from_parts(peer, [TransportAddr::Ip(ip)]);
    let conn = timeout(Duration::from_secs(6), endpoint.connect(target, ALPN)).await??;
    let (mut tx, mut rx) = conn.open_bi().await?;
    tx.write_all(&serde_json::to_vec(&packet)?).await?;
    tx.finish()?;
    let reply = String::from_utf8(timeout(Duration::from_secs(6), rx.read_to_end(MAX_FRAME)).await??)?;
    conn.close(0u32.into(), b"done");
    Ok(reply)
}

async fn send(
    endpoint: &Endpoint,
    host: String,
    key: String,
    id: String,
    text: String,
) -> LabResult<String> {
    if host.len() != 6
        || !host.starts_with("peer")
        || !host[4..].bytes().all(|c| c.is_ascii_digit())
        || text.len() > 1024
        || id.len() > 100
    {
        return Err(
            io::Error::new(io::ErrorKind::InvalidInput, "invalid target or message").into(),
        );
    }
    let remote: EndpointId = key.parse()?;
    let ip = (host.as_str(), PORT)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::AddrNotAvailable, "no target address"))?;
    let target = EndpointAddr::from_parts(remote, [TransportAddr::Ip(ip)]);
    let connection = timeout(Duration::from_secs(6), endpoint.connect(target, ALPN)).await??;
    let (mut tx, mut rx) = connection.open_bi().await?;
    tx.write_all(&serde_json::to_vec(&Message {
        id: id.clone(),
        text,
    })?)
    .await?;
    tx.finish()?;
    let response = String::from_utf8(timeout(Duration::from_secs(6), rx.read_to_end(256)).await??)?;
    connection.close(0u32.into(), b"lab-receipt-read");
    if response != format!("ACK:{id}:STORED") && response != format!("ACK:{id}:DUPLICATE") {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, response).into());
    }
    Ok(response)
}

async fn command(command: Command, endpoint: &Endpoint, state: &State) -> LabResult<String> {
    match command {
        Command::Id => Ok(endpoint.id().to_string()),
        Command::Count => Ok(state.seen.lock().await.len().to_string()),
        Command::Allow { key } => state.allow(key).await,
        Command::Send {
            host, key, id, text,
        } => send(endpoint, host, key, id, text).await,
        Command::RelaySend { host, key, recipient, id, text } => {
            let sender = endpoint.id().to_string();
            let _: EndpointId = recipient.parse()?;
            let data = serde_json::to_vec(&(&sender, &recipient, &id, &text))?;
            let signature = endpoint.secret_key().sign(&data).to_bytes().to_vec();
            let packet = Packet::Store { envelope: Envelope {
                sender, recipient, id: id.clone(), text, signature,
            }};
            let reply = rpc(endpoint, host, key, packet).await?;
            if reply != format!("ACK:{id}:STORED") && reply != format!("ACK:{id}:DUPLICATE") {
                return Err(format!("store refused: {reply}").into());
            }
            Ok(reply)
        }
        Command::Forward { host, key, id } => {
            let env = state.mailbox.lock().await.get(&id).cloned().ok_or("message absent")?;
            let reply = rpc(endpoint, host, key, Packet::Store { envelope: env }).await?;
            if reply != format!("ACK:{id}:STORED") && reply != format!("ACK:{id}:DUPLICATE") {
                return Err(format!("forward refused: {reply}").into());
            }
            Ok(reply)
        }
        Command::Pull { host, key } => {
            let reply = rpc(endpoint, host, key, Packet::Pull).await?;
            let messages: Vec<Envelope> = serde_json::from_str(&reply)?;
            let mut accepted = 0usize;
            for env in messages {
                if env.recipient != endpoint.id().to_string() { return Err("wrong recipient".into()); }
                let source: EndpointId = env.sender.parse()?;
                let sig: [u8;64] = env.signature.as_slice().try_into()?;
                let data = serde_json::to_vec(&(&env.sender, &env.recipient, &env.id, &env.text))?;
                source.verify(&data, &iroh::Signature::from_bytes(&sig))?;
                let status = state.persist(env.sender, Message { id: env.id, text: env.text }).await?;
                if status == "STORED" { accepted += 1; }
                if status != "STORED" && status != "DUPLICATE" {
                    return Err("recipient conflicts with envelope".into());
                }
            }
            Ok(format!("PULLED:{accepted}"))
        }
    }
}

async fn control(mut stream: UnixStream, endpoint: Endpoint, state: Arc<State>) -> LabResult<()> {
    let mut data = Vec::new();
    (&mut stream)
        .take((MAX_FRAME + 1) as u64)
        .read_to_end(&mut data)
        .await?;
    if data.len() > MAX_FRAME {
        return Err("oversize control message".into());
    }
    let result = async {
        let cmd = serde_json::from_slice::<Command>(&data)?;
        command(cmd, &endpoint, &state).await
    }
    .await;
    let reply = match result {
        Ok(response) => response,
        Err(error) => format!("ERROR:{error}"),
    };
    stream.write_all(reply.as_bytes()).await?;
    stream.shutdown().await?;
    Ok(())
}

async fn node() -> LabResult<()> {
    let secret = identity()?;
    let state = Arc::new(State::open()?);
    // No n0 relay, no DNS/Pkarr discovery: ONLY explicitly provided container IPs.
    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(secret)
        .alpns(vec![ALPN.to_vec()])
        .relay_mode(RelayMode::Disabled)
        .clear_address_lookup()
        .clear_ip_transports()
        .bind_addr("0.0.0.0:9444")?
        .bind()
        .await?;
    let _ = fs::remove_file(SOCKET);
    let listener = UnixListener::bind(SOCKET)?;
    println!("READY {}", endpoint.id());
    loop {
        tokio::select! {
            accepted = endpoint.accept() => {
                if let Some(accepted) = accepted {
                    let state = state.clone();
                    tokio::spawn(async move {
                        match accepted.await {
                            Ok(conn) => {
                                if let Err(err) = incoming(conn, state).await {
                                    eprintln!("peer error: {err}");
                                }
                            }
                            Err(err) => eprintln!("accept error: {err}"),
                        }
                    });
                }
            }
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let endpoint = endpoint.clone();
                let state = state.clone();
                tokio::spawn(async move {
                    if let Err(err) = control(stream, endpoint, state).await {
                        eprintln!("control error: {err}");
                    }
                });
            }
        }
    }
}

async fn client(command: Command) -> LabResult<()> {
    let mut stream = UnixStream::connect(SOCKET).await?;
    stream.write_all(&serde_json::to_vec(&command)?).await?;
    stream.shutdown().await?;
    let mut result = String::new();
    stream.read_to_string(&mut result).await?;
    if result.starts_with("ERROR:") {
        return Err(io::Error::other(result).into());
    }
    println!("{result}");
    Ok(())
}

#[tokio::main]
async fn main() -> LabResult<()> {
    let mut args = env::args().skip(1);
    let operation = args.next().unwrap_or_default();
    if operation == "node" {
        return node().await;
    }
    let cmd = match operation.as_str() {
        "id" => Command::Id,
        "count" => Command::Count,
        "allow" => Command::Allow {
            key: args.next().ok_or("missing peer public key")?,
        },
        "send" => Command::Send {
            host: args.next().ok_or("missing peer host")?,
            key: args.next().ok_or("missing peer public key")?,
            id: args.next().ok_or("missing message ID")?,
            text: args.next().ok_or("missing message")?,
        },
        "relay-send" => Command::RelaySend {
            host: args.next().ok_or("missing gateway")?,
            key: args.next().ok_or("missing gateway key")?,
            recipient: args.next().ok_or("missing recipient key")?,
            id: args.next().ok_or("missing id")?,
            text: args.next().ok_or("missing text")?,
        },
        "forward" => Command::Forward {
            host: args.next().ok_or("missing next gateway")?,
            key: args.next().ok_or("missing next key")?,
            id: args.next().ok_or("missing id")?,
        },
        "pull" => Command::Pull {
            host: args.next().ok_or("missing gateway")?,
            key: args.next().ok_or("missing gateway key")?,
        },
        _ => return Err("expected: node | id | count | allow KEY | relay-send HOST KEY RECIPIENT ID TEXT | forward HOST KEY ID | pull HOST KEY".into()),
    };
    client(cmd).await
}
