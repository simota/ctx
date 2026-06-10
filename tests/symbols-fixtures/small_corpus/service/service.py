from auth import User


# Build an active session.
def build_session(user: User) -> str:
    return "token"


# Symbol classification helper.
class Symbol:
    def __init__(self, name, kind):
        self.name = name
        self.kind = kind

    def Render(self):
        return f"{self.name}({self.kind})"


def _private_impl():
    return None
