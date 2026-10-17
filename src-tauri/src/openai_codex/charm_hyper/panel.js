(async (selection) => {
  if (location.origin !== "https://hyper.charm.land") return { error: "wrong_origin" };
  const get = async (path) => {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 8000);
    try {
      const response = await fetch(path, { credentials: "same-origin", redirect: "error", cache: "no-store", headers: { Accept: "application/json" }, signal: controller.signal });
      if (!response.ok || !response.headers.get("content-type")?.includes("application/json")) throw new Error("session_unavailable");
      const text = await response.text();
      if (text.length > 65536) throw new Error("response_too_large");
      return JSON.parse(text);
    } finally { clearTimeout(timer); }
  };
  try {
    const session = await get("/auth/me");
    if (session.authenticated !== true || typeof session.user?.id !== "string") return { error: "login_required" };
    if (selection.userId && session.user.id !== selection.userId) return { error: "different_user" };
    const teams = Array.isArray(session.teams) ? session.teams.map(team => ({ id: team.id, name: team.name })) : [];
    if (teams.length > 100) return { error: "invalid_teams" };
    const teamId = selection.teamId ?? (teams.length === 1 ? teams[0].id : null);
    if (teamId && !teams.some(team => team.id === teamId)) return { error: "different_team" };
    let dashboard = null;
    if (teamId) {
      const data = await get(`/teams/${encodeURIComponent(teamId)}/dashboard`);
      dashboard = Object.fromEntries(["now", "window_minutes", "subscribed", "plan_name", "limited", "known", "available", "remaining", "cap", "refresh_at", "returns_at", "allowance", "refresh_hours"].map(key => [key, data[key]]).filter(([, value]) => value !== undefined));
    }
    return { userId: session.user.id, teams, teamId, dashboard };
  } catch { return { error: "session_unavailable" }; }
})(__JARVIS_HYPER_SELECTION__)
