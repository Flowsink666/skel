from typing import Optional, List
from datetime import datetime

class User:
    """User entity representing account credentials and metadata."""
    def __init__(self, user_id: str, username: str, email: str, role: str = "member"):
        self.user_id = user_id
        self.username = username
        self.email = email
        self.role = role
        self.created_at = datetime.utcnow()

    def to_dict(self) -> dict:
        """Convert user instance to serialization dictionary."""
        return {
            "user_id": self.user_id,
            "username": self.username,
            "email": self.email,
            "role": self.role,
            "created_at": self.created_at.isoformat()
        }


class Organization:
    """Organization model managing member workspaces and quotas."""
    def __init__(self, org_id: str, name: str, owner_id: str):
        self.org_id = org_id
        self.name = name
        self.owner_id = owner_id
        self.members: List[str] = [owner_id]
        self.tier: str = "pro"

    def add_member(self, user_id: str) -> bool:
        """Add a member if under organization plan quota limit."""
        if len(self.members) >= 50 and self.tier != "enterprise":
            raise ValueError("Member limit reached for tier")
        self.members.append(user_id)
        return True
