# quasar-lobby

Matchmaking and session management for the Quasar Engine.

## Features

- **Session Management**: Create, list, join, leave
- **Player Management**: Ready state, teams, metadata
- **Matchmaking**: Filter-based session search
- **REST API**: HTTP endpoints
- **WebSocket Events**: Real-time updates

## Usage

```rust
use quasar_lobby::LobbyServer;

let server = LobbyServer::new(LobbyServerConfig::default()).await?;
server.run().await?;
```

## API Endpoints

| Method | Endpoint                   | Description     |
| ------ | -------------------------- | --------------- |
| GET    | `/api/sessions`            | List sessions   |
| POST   | `/api/sessions`            | Create session  |
| GET    | `/api/sessions/{id}`       | Get session     |
| POST   | `/api/sessions/{id}/join`  | Join session    |
| POST   | `/api/sessions/{id}/leave` | Leave session   |
| DELETE | `/api/sessions/{id}`       | Destroy session |

## WebSocket Events

- `SessionCreated`
- `SessionDestroyed`
- `PlayerJoined`
- `PlayerLeft`
