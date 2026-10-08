"""Run upstream Backlot for one authorized production without detached processes."""
import sys
from pathlib import Path

import uvicorn
from fastapi import HTTPException
from starlette.middleware.trustedhost import TrustedHostMiddleware
from backlot import server

production = Path(sys.argv[1]).resolve(strict=True)
port = int(sys.argv[2])


def authorized_project(project_id):
    if project_id != production.name:
        raise HTTPException(status_code=404, detail="Unknown production")
    return production


def selected_change(path):
    candidate = Path(path).resolve()
    if not candidate.is_relative_to(production):
        return None
    if server._IGNORE_PARTS.intersection(candidate.relative_to(production).parts):
        return None
    return production.name


server.PROJECTS_DIR = production
server._safe_project_dir = authorized_project
server._project_of_change = selected_change
server._cached_summaries = lambda: [server.summarize_project(production)]
server.app.add_middleware(TrustedHostMiddleware, allowed_hosts=["127.0.0.1"])
uvicorn.run(server.app, host="127.0.0.1", port=port, log_level="warning")
