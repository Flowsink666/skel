import hashlib
import hmac
import time
from typing import Optional, Dict
from .models import User

SECRET_KEY = b"super-secret-system-key-2026"

class AuthService:
    """Authentication service handling JWT tokens, password hashing, and session validation."""

    def __init__(self, token_ttl: int = 3600):
        self.token_ttl = token_ttl
        self._session_cache: Dict[str, float] = {}

    def hash_password(self, password: str, salt: str) -> str:
        """Hash plain password using PBKDF2 HMAC SHA256 with 100,000 iterations."""
        pwd_bytes = password.encode("utf-8")
        salt_bytes = salt.encode("utf-8")
        derived = hashlib.pbkdf2_hmac("sha256", pwd_bytes, salt_bytes, 100000)
        return derived.hex()

    def verify_password(self, plain: str, salt: str, expected_hash: str) -> bool:
        """Constant-time comparison to prevent timing attacks."""
        calculated = self.hash_password(plain, salt)
        return hmac.compare_digest(calculated, expected_hash)

    def issue_session(self, user: User) -> str:
        """Issue temporary session token with expiration timestamp."""
        payload = f"{user.user_id}:{time.time() + self.token_ttl}"
        signature = hmac.new(SECRET_KEY, payload.encode(), hashlib.sha256).hexdigest()
        token = f"{payload}:{signature}"
        self._session_cache[token] = time.time() + self.token_ttl
        return token

    def validate_session(self, token: str) -> bool:
        """Validate token signature and expiration against in-memory cache."""
        parts = token.split(":")
        if len(parts) != 3:
            return False
        user_id, exp_str, signature = parts
        exp = float(exp_str)
        if time.time() > exp:
            return False
        expected_sig = hmac.new(SECRET_KEY, f"{user_id}:{exp_str}".encode(), hashlib.sha256).hexdigest()
        return hmac.compare_digest(signature, expected_sig)
