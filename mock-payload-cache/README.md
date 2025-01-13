# Payload Cache Service

This repository provides a simple service for storing and retrieving hex-encoded payloads. It uses a keccak256 hash as the lookup key.

---

## Installation

1. **Clone this repository**:

2. **Install dependencies**:

   ```bash
   npm install
   ```
   
3. **Set up environment variables**:  
   - Create a `.env` file in the project root and add `PAYLOAD_CACHE_AUTH_TOKEN`.

4. **Run the server**:

   ```bash
   npm start
   ```

---

## Configuration

### Required Environment Variables

| Variable                       | Description                                            |
|-------------------------------|--------------------------------------------------------|
| `PAYLOAD_CACHE_AUTH_TOKEN`    | Bearer token used to authenticate incoming requests    |

---

## Usage

After starting the server, it will listen on port `5001` by default.  

- **Swagger UI**: Visit `http://localhost:5001/docs` to view API documentation.  
- **Authorization**: All API calls must include the header `Authorization: Bearer <TOKEN>`, where `<TOKEN>` matches `PAYLOAD_CACHE_AUTH_TOKEN` in your `.env`.

---

## API Endpoints

### POST `/payload`

**Description**  
Stores a hex-encoded payload in memory by computing its keccak256 hash.

**Headers**  
```
Authorization: Bearer <PAYLOAD_CACHE_AUTH_TOKEN>
Content-Type: text/plain
```

**Request Body**  
A hex-encoded string representing the payload. For example:  
```
68656c6c6f776f726c64
```

**Response**  
- **200 OK**: Returns a JSON object containing the hash of the payload.
  ```json
  {
    "hash": "<keccak256_hash>"
  }
  ```
- **400 Bad Request**: If the payload is missing, not a string, or not valid hex.
- **403 Forbidden**: If the `Authorization` header is missing or incorrect.

---

### GET `/payload`

**Description**  
Retrieves the original hex-encoded payload for a given keccak256 hash.

**Headers**  
```
Authorization: Bearer <PAYLOAD_CACHE_AUTH_TOKEN>
```

**Query Parameters**  
- **hash** (required): The keccak256 hash (hex string) used to look up the payload.

**Response**  
- **200 OK**: Returns the payload in plain text if found.
- **400 Bad Request**: If the hash query parameter is missing.
- **404 Not Found**: If no payload is stored under the provided hash.
- **403 Forbidden**: If the `Authorization` header is missing or incorrect.

---

### GET `/debug`

**Description**  
Returns the entire in-memory store as JSON for debugging.

**Headers**  
```
Authorization: Bearer <PAYLOAD_CACHE_AUTH_TOKEN>
```

**Response**  
- **200 OK**: Returns a JSON object containing all hashes and their payloads.
- **403 Forbidden**: If the `Authorization` header is missing or incorrect.

---
