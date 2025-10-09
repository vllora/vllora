# Session Authentication API Documentation

## Overview
This API enables CLI authentication flow by creating temporary sessions and exchanging them for API keys through a web browser login.

## Authentication Flow

1. **CLI initiates session** → Calls `POST /session/start`
2. **User opens browser** → Navigates to UI with session_id
3. **User logs in** → UI authenticates user and associates session with API key
4. **CLI polls for key** → Calls `GET /session/fetch_key/{session_id}` repeatedly until key is available
5. **CLI receives key** → Saves credentials locally

## API Endpoints

### 1. Start Session
**Endpoint:** `POST /session/start`

**Description:** Creates a new session and returns a unique session_id

**Request:**
```bash
curl -X POST http://localhost:8080/session/start
```

**Response:** `200 OK`
```json
{
  "session_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**Error Responses:**
- `500 Internal Server Error` - Failed to create session on backend

---

### 2. Fetch API Key
**Endpoint:** `GET /session/fetch_key/{session_id}`

**Description:** Retrieves the API key for a session (if available). This endpoint does NOT retry internally - retry logic must be implemented on the frontend.

**Request:**
```bash
curl http://localhost:8080/session/fetch_key/550e8400-e29b-41d4-a716-446655440000
```

**Response:** `200 OK`
```json
{
  "api_key": "lgdb_1234567890abcdef"
}
```

**Response:** `404 Not Found`
- Session not found or API key not yet created
- Frontend should retry after a delay

**Error Responses:**
- `500 Internal Server Error` - Failed to fetch key from backend

---

## Frontend Implementation Guide

### Required Component Features

1. **Session Initialization**
   - Call `POST /session/start` when user clicks "Login" or similar
   - Store the returned `session_id`
   - Generate the login URL: `${LANGDB_UI_URL}/login?session_id=${session_id}`
   - Open URL in browser (new tab or popup)

2. **Polling for API Key**
   - Start polling `GET /session/fetch_key/{session_id}` immediately after opening browser
   - **Polling Configuration:**
     - Interval: 1 second (1000ms)
     - Timeout: 120 seconds (2 minutes)
     - Stop conditions:
       - Receive `200 OK` with API key → Success
       - Timeout reached → Show error
       - User cancels → Stop polling

3. **UI States**
   ```
   IDLE → WAITING_FOR_LOGIN → SUCCESS | TIMEOUT | ERROR
   ```

4. **Error Handling**
   - `404 Not Found`: Continue polling (normal state)
   - `500 Internal Server Error`: Show error, optionally retry
   - Timeout: Show "Login timeout. Please try again."
   - Network errors: Show "Connection failed. Please check your network."

### Example Implementation Flow

```javascript
async function login() {
  // 1. Start session
  const { session_id } = await fetch('/session/start', { method: 'POST' })
    .then(r => r.json());
  
  // 2. Open browser for user to log in
  const loginUrl = `${LANGDB_UI_URL}/login?session_id=${session_id}`;
  window.open(loginUrl, '_blank');
  
  // 3. Poll for API key
  const startTime = Date.now();
  const timeout = 120000; // 2 minutes
  
  while (Date.now() - startTime < timeout) {
    try {
      const response = await fetch(`/session/fetch_key/${session_id}`);
      
      if (response.ok) {
        const { api_key } = await response.json();
        // Success! Save API key
        saveCredentials(api_key);
        return { success: true, api_key };
      }
      
      if (response.status === 404) {
        // Not ready yet, continue polling
        await sleep(1000);
        continue;
      }
      
      // Other error
      throw new Error(`Failed to fetch key: ${response.status}`);
      
    } catch (error) {
      console.error('Polling error:', error);
      await sleep(1000);
    }
  }
  
  // Timeout
  return { success: false, error: 'Login timeout' };
}
```

### Environment Variables

- `LANGDB_UI_URL` - The UI base URL for login (defaults to production if not set)
- Base URL for API calls depends on your gateway configuration

### Session Lifecycle

- **Session TTL:** 2 minutes (120 seconds)
- Sessions expire automatically after TTL
- Sessions are single-use (API key retrieved once)

### Security Considerations

- Sessions are tied to client IP on the backend
- Sessions expire after 2 minutes
- Rate limiting is enforced on backend
- Always use HTTPS in production

---

## Testing

### Manual Testing Flow

1. Start gateway: `cargo run`
2. Call start session:
   ```bash
   curl -X POST http://localhost:8080/session/start
   ```
3. Note the `session_id` from response
4. Poll for key (should return 404 until initialized):
   ```bash
   curl http://localhost:8080/session/fetch_key/{session_id}
   ```
5. Complete login flow in UI
6. Poll again - should return 200 with API key

### Expected Responses

```bash
# Before login
GET /session/fetch_key/{session_id} → 404 Not Found

# After login
GET /session/fetch_key/{session_id} → 200 OK
{
  "api_key": "lgdb_..."
}
```

