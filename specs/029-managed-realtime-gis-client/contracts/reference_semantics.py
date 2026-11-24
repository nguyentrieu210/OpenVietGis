"""Reference contract only; not production persistence or API."""
from dataclasses import dataclass

@dataclass(frozen=True)
class Scope:
    organization: str
    project: str
    session: str
    epoch: int

@dataclass(frozen=True)
class Cursor:
    scope: Scope
    position: int

class ResetRequired(Exception):
    pass

def advance(cursor, scope, floor, high, scanned_through, applied):
    """Advance after successful atomic apply; visible ID gaps are legitimate."""
    if cursor.scope != scope or not floor <= cursor.position <= high:
        raise ResetRequired()
    if not cursor.position <= scanned_through <= high:
        raise ValueError("invalid scan range")
    return Cursor(scope, scanned_through) if applied else cursor

def receipt_match(stored_actor, stored_digest, actor, digest):
    if stored_actor != actor or stored_digest != digest:
        raise ValueError("operation identity mismatch")
    return True

def save_state(authorized, connected, receipt):
    if not authorized:
        return "denied"
    if not connected or receipt is None:
        return "unconfirmed"
    return "accepted" if receipt == "durable" else "conflict"
