# quasar-network

Networking module for the Quasar Engine.

## Features

- **UDP Transport**: Low-latency packet transport
- **QUIC Transport**: Reliable UDP via Quinn (optional)
- **Delta Compression**: Bandwidth-efficient state sync
- **Interest Management**: Only send relevant data
- **Client Prediction**: Smooth client movement
- **Rollback**: Server-authoritative with client prediction
- **Security**: Rate limiting, message validation

## Usage

```rust
use quasar_network::NetworkPlugin;

app.add_plugin(NetworkPlugin);

// Client
let client = NetworkClient::connect("127.0.0.1:7777")?;

// Server
let server = NetworkServer::bind("0.0.0.0:7777")?;
```

## Features

| Feature           | Description             |
| ----------------- | ----------------------- |
| `quinn-transport` | QUIC reliable transport |

## Protocol

- Sequence numbers for ordering
- Delta compression for bandwidth
- Interest sets for relevance
