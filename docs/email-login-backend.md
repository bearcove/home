# Email Login Backend Documentation

This document describes the email-based authentication system implemented in the home backend (mom and cub services).

## Overview

The email login system allows users to authenticate using their email address. The flow is:
1. User enters their email address
2. System sends a 6-digit code to their email
3. User enters the code to complete login
4. System creates an auth session valid for 30 days

## Backend Architecture

### Database Schema (SQLite in mom)

```sql
CREATE TABLE email_login_codes (
    id TEXT NOT NULL PRIMARY KEY,
    email TEXT NOT NULL,
    code TEXT NOT NULL,
    created_at DATETIME NOT NULL,
    expires_at DATETIME NOT NULL,
    used_at DATETIME,
    ip_address TEXT,
    user_agent TEXT
);

CREATE INDEX idx_email_login_codes_email ON email_login_codes(email);
CREATE INDEX idx_email_login_codes_code ON email_login_codes(code);
CREATE INDEX idx_email_login_codes_expires_at ON email_login_codes(expires_at);
```

### API Endpoints

#### 1. Generate Login Code
- **Mom endpoint**: `POST /tenant/{tenant_name}/email/generate-code`
- **Request body**:
  ```json
  {
    "email": "user@example.com"
  }
  ```
- **Response**:
  ```json
  {
    "code": "123456",
    "expires_at": "2024-01-01T12:15:00Z"
  }
  ```
- **Notes**: 
  - Code is 6 digits
  - Expires in 15 minutes
  - Email is sent if SMTP is configured

#### 2. Validate Login Code
- **Mom endpoint**: `POST /tenant/{tenant_name}/email/validate-code`
- **Request body**:
  ```json
  {
    "email": "user@example.com",
    "code": "123456",
    "ip_address": "1.2.3.4",  // optional
    "user_agent": "Mozilla/5.0..."  // optional
  }
  ```
- **Response**:
  ```json
  {
    "auth_bundle": {
      "user_info": {
        "profile": {
          "full_name": "user@example.com",
          "thumb_url": "https://www.gravatar.com/avatar/...",
          "github_id": null,
          "patreon_id": null
        },
        "tier": null
      },
      "expires_at": "2024-02-01T12:00:00Z"
    }
  }
  ```

### Cub (Frontend) Endpoints

#### 1. Email Login Page
- **Route**: `GET /login/email`
- **Query params**: `?return_to=/some/path` (optional)
- **Template**: `login-email.html`
- **Context variables**:
  - `return_to`: Where to redirect after login
  - `error`: Error message (if any)
  - `email`: Pre-filled email (on error)

#### 2. Submit Email
- **Route**: `POST /login/email`
- **Form data**:
  - `email`: User's email address
  - `return_to`: Optional redirect path
- **Flow**:
  1. Calls mom's generate-code endpoint
  2. Stores email in cookie (`email_login`)
  3. Redirects to `/login/email/verify`
  4. On error: Re-renders form with error message

#### 3. Code Verification Page
- **Route**: `GET /login/email/verify`
- **Template**: `login-email-verify.html`
- **Context variables**:
  - `email`: The email address (from cookie)
  - `error`: Error message (if any)
- **Note**: Redirects to `/login/email` if no email cookie found

#### 4. Submit Verification Code
- **Route**: `POST /login/email/verify`
- **Form data**:
  - `code`: The 6-digit code
- **Flow**:
  1. Gets email from cookie
  2. Extracts IP and user-agent from headers
  3. Calls mom's validate-code endpoint
  4. On success:
     - Sets auth cookie (`home-credentials`)
     - Sets `just_logged_in` cookie for JS events
     - Clears `email_login` cookie
     - Redirects to `return_to` path or `/`
  5. On error: Re-renders form with error message

## Email Configuration

Email sending requires SMTP configuration in mom's config:

```json
{
  "secrets": {
    "email": {
      "smtp_host": "smtp.example.com",
      "smtp_port": 587,
      "smtp_username": "noreply@example.com",
      "smtp_password": "secret",
      "from_email": "noreply@example.com",
      "from_name": "Your Site Name"
    }
  }
}
```

If email is not configured, the system will still work but won't send emails (useful for development).

## UI Templates Required

### 1. `login-email.html`
Should contain:
- Email input field (name="email")
- Hidden return_to field if provided
- Submit button
- Error display if `error` context variable is set
- Link to other login methods

Example form:
```html
<form method="post" action="/login/email">
  <input type="email" name="email" value="{{ email }}" required>
  <input type="hidden" name="return_to" value="{{ return_to }}">
  <button type="submit">Send Login Code</button>
</form>
{% if error %}
  <div class="error">{{ error }}</div>
{% endif %}
```

### 2. `login-email-verify.html`
Should contain:
- Display of email address being verified
- Code input field (name="code", pattern="[0-9]{6}")
- Submit button
- Error display if `error` context variable is set
- Link to go back/resend code

Example form:
```html
<p>Enter the code sent to {{ email }}:</p>
<form method="post" action="/login/email/verify">
  <input type="text" name="code" pattern="[0-9]{6}" maxlength="6" required>
  <button type="submit">Verify Code</button>
</form>
{% if error %}
  <div class="error">{{ error }}</div>
{% endif %}
<a href="/login/email">Use a different email</a>
```

## Security Considerations

1. **Rate Limiting**: Not implemented yet, but should limit code generation per email
2. **Code Expiry**: 15 minutes
3. **Code Usage**: Codes are marked as used and cannot be reused
4. **IP/User-Agent Tracking**: Stored for security auditing
5. **Cookie Security**: 
   - Email cookie expires in 15 minutes
   - Uses private (encrypted) cookies
   - Path set to `/` for all cookies

## User Profile Details

Email login creates a minimal user profile:
- `full_name`: Set to the email address
- `thumb_url`: Gravatar URL based on email hash
- `tier`: None (no tier access by default)
- No GitHub or Patreon IDs

The auth session expires after 30 days.

## Integration with Existing Auth

The email login system integrates seamlessly with existing GitHub and Patreon logins:
- Uses the same `AuthBundle` structure
- Sets the same `home-credentials` cookie
- Triggers the same `just_logged_in` JS event
- Uses the same `return_to` cookie mechanism
- Shares the same logout endpoint

## Development Notes

- In development, if email is not configured, the code is still returned in the API response (but not sent via email)
- The system uses time-based code generation with nanosecond precision to ensure uniqueness
- Gravatar is used for profile pictures with the `identicon` fallback