# counter

Server template that provides an interface to track a number count.

Currently serving using CloudFlare nameservers and a `cloudflared` tunnel with a Route53 domain...

## API Documentation

### Endpoints

#### `GET /hit`
Get the current counter state without modifying it.

**Response:**
```
{
  "count": 2,
  "max": 3,
  "saturated": false
}
```

---

#### `POST /hit`
Increment the counter by 1 (saturates at max, won't exceed it).

**Response:**
```
{
  "count": 3,
  "saturated": true
}
```

---

#### `GET /max`
Get the current maximum value.

**Response:**
```
{
  "max": 3
}
```

---

#### `POST /max`
Update the maximum value (clamps current count if new max is lower).

**Request:**
```
{
  "max": 10
}
```

**Response:**
```
{
  "max": 10
}
```

**Errors:**
- `max` must be greater than 0
- Returns 400 Bad Request if invalid

---

#### `POST /reset`
Reset the counter to 0 (keeps the same maximum).

**Response:**
```
{
  "count": 0,
  "max": 3
}
```

---

### Error Responses

All errors return JSON with an `error` field:

```
{
  "error": "counter operation failed - max must be greater than 0, got 0"
}
```
