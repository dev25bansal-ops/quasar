# Lobby Server

A matchmaking server for multiplayer games.

## Features

- Session management
- Player matchmaking
- REST API
- WebSocket events

## Running

```bash
cargo run -p lobby_server
```

Server listens on `http://127.0.0.1:8080`

## API

### REST Endpoints

| Method | Endpoint                   | Description     |
| ------ | -------------------------- | --------------- |
| GET    | `/health`                  | Health check    |
| GET    | `/api/sessions`            | List sessions   |
| POST   | `/api/sessions`            | Create session  |
| GET    | `/api/sessions/{id}`       | Get session     |
| POST   | `/api/sessions/{id}/join`  | Join session    |
| POST   | `/api/sessions/{id}/leave` | Leave session   |
| DELETE | `/api/sessions/{id}`       | Destroy session |

### Example

```bash
# Create session
curl -X POST http://localhost:8080/api/sessions \
  -H "Content-Type: application/json" \
  -d '{"config": {"max_players": 4}, "player_id": "player-1"}'

# List sessions
curl http://localhost:8080/api/sessions
```

## Configuration

Edit `LobbyServerConfig` for:

- Port binding
- Max sessions
- Timeout settings
