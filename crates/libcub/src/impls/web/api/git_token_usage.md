# Git Clone Token Usage

## How to generate a token

Make a POST request to `/api/git-token` with the repository name:

```bash
curl -X POST https://your-domain.com/api/git-token \
  -H "Content-Type: application/json" \
  -H "Cookie: your-auth-cookie" \
  -d '{"repo": "my-repo"}'
```

Response:
```json
{
  "token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9...",
  "clone_url": "https://your-domain.com/extras/my-repo.git",
  "expires_in": 86400
}
```

## How to use the token

Use the token as the password with "token" as the username:

```bash
git clone https://token:YOUR_JWT_TOKEN@your-domain.com/extras/my-repo.git
```

The token expires after 24 hours (86400 seconds).

## Requirements

- User must be authenticated (have a valid session)
- User must have Bronze tier or higher
- Token is specific to the tenant (uses tenant's cookie_sauce for signing)