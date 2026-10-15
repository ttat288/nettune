//! The single automatic optimization NetTune performs: the per-connection
//! NetworkManager Wi-Fi power-save setting (`802-11-wireless.powersave`).
//!
//! Safety properties:
//! - Only NetworkManager is asked to change its own profile (`nmcli`), so polkit
//!   authorizes exactly this operation; NetTune never runs as root.
//! - Arguments are validated (UUID, interface name, value whitelist) and passed
//!   as an argument vector, never through a shell.
//! - The previous value is written to a backup file *before* anything changes.
//! - A failed activation triggers an immediate restore of the previous value.
//! - The result is verified by reading back the profile and the driver state.

use crate::cmd::{split_nmcli_terse, CmdError, CmdOutput, Runner};
use crate::detect::parse_iw_power_save;
use crate::models::{InterfaceKind, NetInterface, PowerSaveBackup, PowerSaveState};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const READ_T: Duration = Duration::from_secs(10);
/// Modifications may wait on a polkit password dialog.
const MODIFY_T: Duration = Duration::from_secs(120);
const UP_WAIT_S: &str = "30";
const UP_T: Duration = Duration::from_secs(45);

pub const DISABLE: &str = "disable";
const VALID_VALUES: &[&str] = &["default", "ignore", "disable", "enable"];

pub fn valid_uuid(u: &str) -> bool {
    u.len() == 36
        && u.chars().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

pub fn valid_ifname(n: &str) -> bool {
    !n.is_empty()
        && n.len() <= 15
        && !n.starts_with('-')
        && n.chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}

/// nmcli prints the numeric form in some versions ("2 (disable)"); normalise to the name.
pub fn normalize_value(v: &str) -> Option<String> {
    let v = v.trim();
    let name = match v.split_whitespace().next()? {
        "0" => "default",
        "1" => "ignore",
        "2" => "disable",
        "3" => "enable",
        _ => v
            .trim_start_matches(|c: char| c.is_ascii_digit() || c == ' ' || c == '(')
            .trim_end_matches(')'),
    };
    VALID_VALUES.contains(&name).then(|| name.to_string())
}

// ---------------------------------------------------------------- backup store

pub struct BackupStore {
    path: PathBuf,
}

impl BackupStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// `$XDG_STATE_HOME/nettune/wifi-powersave-backup.json` (default `~/.local/state`).
    pub fn default_location() -> Self {
        let base = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
            .unwrap_or_else(std::env::temp_dir);
        Self::new(base.join("nettune").join("wifi-powersave-backup.json"))
    }

    pub fn load(&self) -> Option<PowerSaveBackup> {
        let b: PowerSaveBackup =
            serde_json::from_str(&fs::read_to_string(&self.path).ok()?).ok()?;
        (valid_uuid(&b.connection_uuid)
            && valid_ifname(&b.interface)
            && VALID_VALUES.contains(&b.previous_value.as_str()))
        .then_some(b)
    }

    pub fn save(&self, b: &PowerSaveBackup) -> Result<(), String> {
        let dir = self.path.parent().ok_or("invalid backup path")?;
        fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let tmp = self.path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(b).map_err(|e| e.to_string())?;
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts
            .open(&tmp)
            .map_err(|e| format!("cannot write backup: {e}"))?;
        f.write_all(json.as_bytes())
            .and_then(|_| f.sync_all())
            .map_err(|e| format!("cannot write backup: {e}"))?;
        fs::rename(&tmp, &self.path).map_err(|e| format!("cannot write backup: {e}"))
    }

    pub fn clear(&self) {
        let _ = fs::remove_file(&self.path);
    }
}

// ---------------------------------------------------------------- reading state

fn nmcli(r: &dyn Runner, args: &[&str], t: Duration) -> Result<CmdOutput, String> {
    r.run("nmcli", args, t).map_err(|e| match e {
        CmdError::NotFound(_) => "NetworkManager's `nmcli` is not installed".to_string(),
        other => other.to_string(),
    })
}

fn err_text(o: &CmdOutput) -> String {
    let s = o.stderr.trim();
    let s = s.strip_prefix("Error: ").unwrap_or(s);
    if s.is_empty() {
        format!("nmcli exited with status {:?}", o.status)
    } else {
        s.lines().next().unwrap_or(s).to_string()
    }
}

struct Profile {
    conn_type: String,
    owned_by_user: bool,
    powersave: Option<String>,
}

fn read_profile(r: &dyn Runner, uuid: &str) -> Result<Profile, String> {
    let o = nmcli(
        r,
        &[
            "-g",
            "connection.type,connection.permissions,802-11-wireless.powersave",
            "connection",
            "show",
            uuid,
        ],
        READ_T,
    )?;
    if !o.success() {
        return Err(err_text(&o));
    }
    let lines: Vec<&str> = o.stdout.lines().collect();
    Ok(Profile {
        conn_type: lines
            .first()
            .map(|s| s.trim().to_string())
            .unwrap_or_default(),
        owned_by_user: lines.get(1).is_some_and(|s| !s.trim().is_empty()),
        powersave: lines.get(2).and_then(|s| normalize_value(s)),
    })
}

fn read_runtime(r: &dyn Runner, ifname: &str) -> Option<bool> {
    let o = r
        .run("iw", &["dev", ifname, "get", "power_save"], READ_T)
        .ok()?;
    parse_iw_power_save(&o.stdout)
}

fn read_auth(r: &dyn Runner, own: bool) -> Option<String> {
    let o = nmcli(
        r,
        &["-t", "-f", "PERMISSION,VALUE", "general", "permissions"],
        READ_T,
    )
    .ok()?;
    let want = if own {
        "org.freedesktop.NetworkManager.settings.modify.own"
    } else {
        "org.freedesktop.NetworkManager.settings.modify.system"
    };
    o.stdout.lines().find_map(|l| {
        let f = split_nmcli_terse(l);
        (f.first().map(String::as_str) == Some(want)).then(|| f.get(1).cloned().unwrap_or_default())
    })
}

pub fn state(iface: &NetInterface, r: &dyn Runner, store: &BackupStore) -> PowerSaveState {
    let backup = store.load().filter(|b| b.interface == iface.name);
    let mut st = PowerSaveState {
        supported: false,
        unsupported_reason: None,
        profile_value: None,
        runtime_on: None,
        connection_uuid: iface.connection_uuid.clone(),
        auth: None,
        backup,
    };
    let unsupported = |mut st: PowerSaveState, why: &str| {
        st.unsupported_reason = Some(why.to_string());
        st
    };
    if iface.kind != InterfaceKind::Wifi {
        return unsupported(st, "Not a Wi-Fi interface.");
    }
    if !valid_ifname(&iface.name) {
        return unsupported(st, "Unexpected interface name.");
    }
    st.runtime_on = read_runtime(r, &iface.name);
    if !iface.nm_managed {
        return unsupported(st, "This interface is not managed by NetworkManager.");
    }
    let Some(uuid) = iface.connection_uuid.as_deref().filter(|u| valid_uuid(u)) else {
        return unsupported(st, "No active NetworkManager connection on this interface.");
    };
    let profile = match read_profile(r, uuid) {
        Ok(p) => p,
        Err(e) => return unsupported(st, &format!("Could not read the connection profile: {e}")),
    };
    if profile.conn_type != "802-11-wireless" {
        return unsupported(st, "The active connection is not a Wi-Fi profile.");
    }
    st.profile_value = profile.powersave;
    if st.profile_value.is_none() {
        return unsupported(st, "The profile's power-save value could not be read.");
    }
    if st.runtime_on.is_none() {
        return unsupported(
            st,
            "The Wi-Fi driver does not report its power-saving state, so a change could not be verified.",
        );
    }
    st.auth = read_auth(r, profile.owned_by_user);
    match st.auth.as_deref() {
        Some("yes") | Some("auth") => {}
        Some(_) => {
            return unsupported(
                st,
                "System policy (polkit) does not allow changing this connection.",
            )
        }
        None => return unsupported(st, "Could not determine NetworkManager permissions."),
    }
    st.supported = true;
    st
}

// ---------------------------------------------------------------- changing state

fn set_value(r: &dyn Runner, uuid: &str, value: &str) -> Result<(), String> {
    let o = nmcli(
        r,
        &[
            "connection",
            "modify",
            uuid,
            "802-11-wireless.powersave",
            value,
        ],
        MODIFY_T,
    )?;
    if o.success() {
        Ok(())
    } else {
        Err(err_text(&o))
    }
}

fn activate(r: &dyn Runner, uuid: &str) -> Result<(), String> {
    let o = nmcli(r, &["--wait", UP_WAIT_S, "connection", "up", uuid], UP_T)?;
    if o.success() {
        Ok(())
    } else {
        Err(err_text(&o))
    }
}

fn is_active(r: &dyn Runner, uuid: &str) -> bool {
    nmcli(r, &["-g", "UUID", "connection", "show", "--active"], READ_T)
        .map(|o| o.stdout.lines().any(|l| l.trim() == uuid))
        .unwrap_or(false)
}

fn wait_runtime(r: &dyn Runner, ifname: &str, want: bool, settle: Duration) -> Option<bool> {
    let mut last = None;
    for _ in 0..6 {
        last = read_runtime(r, ifname);
        if last == Some(want) {
            break;
        }
        std::thread::sleep(settle);
    }
    last
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Debug, PartialEq)]
pub struct Outcome {
    /// The change is in place and verified.
    pub ok: bool,
    pub message: String,
}

/// Disable power saving on the interface's active connection.
pub fn apply(
    iface: &NetInterface,
    r: &dyn Runner,
    store: &BackupStore,
    settle: Duration,
) -> Result<Outcome, String> {
    let st = state(iface, r, store);
    if !st.supported {
        return Err(st
            .unsupported_reason
            .unwrap_or_else(|| "Not supported on this interface.".into()));
    }
    if store.load().is_some() {
        return Err("A previous NetTune change is still recorded. Revert it first.".into());
    }
    let uuid = st.connection_uuid.clone().unwrap_or_default();
    let previous = st.profile_value.clone().unwrap_or_default();
    if previous == DISABLE {
        return Err("Power saving is already disabled in this connection profile.".into());
    }

    // 1. Record the previous value before touching anything.
    store.save(&PowerSaveBackup {
        interface: iface.name.clone(),
        connection_uuid: uuid.clone(),
        previous_value: previous.clone(),
        applied_value: DISABLE.into(),
        applied_at_unix: now_unix(),
    })?;

    // 2. Change the profile. On refusal nothing changed: drop the backup.
    if let Err(e) = set_value(r, &uuid, DISABLE) {
        store.clear();
        return Err(format!("NetworkManager did not accept the change: {e}"));
    }

    // 3. Re-activate so the driver picks it up. On failure restore immediately.
    if let Err(e) = activate(r, &uuid) {
        let restored = set_value(r, &uuid, &previous).is_ok();
        let _ = activate(r, &uuid);
        if restored {
            store.clear();
            return Err(format!(
                "Re-activating the connection failed ({e}). The previous setting \"{previous}\" was restored."
            ));
        }
        return Err(format!(
            "Re-activating the connection failed ({e}) and the previous setting could not be restored automatically. Use Revert."
        ));
    }

    // 4. Verify.
    let profile_now = read_profile(r, &uuid).ok().and_then(|p| p.powersave);
    let runtime = wait_runtime(r, &iface.name, false, settle);
    match (profile_now.as_deref(), runtime) {
        (Some(DISABLE), Some(false)) => Ok(Outcome {
            ok: true,
            message: "Power saving disabled and verified: the driver reports power save off. Run diagnostics again to compare.".into(),
        }),
        (Some(DISABLE), other) => Ok(Outcome {
            ok: false,
            message: format!(
                "The profile was changed, but the driver still reports power save {}. The driver may override this setting. You can revert.",
                match other {
                    Some(true) => "on",
                    _ => "as unknown",
                }
            ),
        }),
        _ => Ok(Outcome {
            ok: false,
            message: "The change could not be verified in the profile. You can revert.".into(),
        }),
    }
}

/// Restore the value recorded in the backup.
pub fn revert(r: &dyn Runner, store: &BackupStore, settle: Duration) -> Result<Outcome, String> {
    let b = store
        .load()
        .ok_or("There is no NetTune change to revert.")?;
    // Confirm the profile still exists before changing anything.
    if let Err(e) = read_profile(r, &b.connection_uuid) {
        let lower = e.to_lowercase();
        if lower.contains("no such connection") || lower.contains("unknown connection") {
            store.clear();
            return Err("The connection profile no longer exists; nothing to revert.".into());
        }
        return Err(format!("Could not read the connection profile: {e}"));
    }
    set_value(r, &b.connection_uuid, &b.previous_value)
        .map_err(|e| format!("NetworkManager did not accept the change: {e}"))?;
    let mut message = format!("Restored power-save setting \"{}\".", b.previous_value);
    let active = is_active(r, &b.connection_uuid);
    if active {
        if let Err(e) = activate(r, &b.connection_uuid) {
            message.push_str(&format!(
                " Re-activation failed ({e}); it will apply on the next connection."
            ));
        }
    } else {
        message.push_str(" The connection is not active; it will apply on the next connection.");
    }
    let now = read_profile(r, &b.connection_uuid)
        .ok()
        .and_then(|p| p.powersave);
    if now.as_deref() != Some(b.previous_value.as_str()) {
        return Ok(Outcome {
            ok: false,
            message:
                "The profile did not report the restored value. The backup was kept; try again."
                    .into(),
        });
    }
    if active {
        // Only meaningful when the restored profile is the one driving the interface.
        if let Some(on) = wait_runtime(r, &b.interface, b.previous_value != "disable", settle) {
            message.push_str(&format!(
                " Driver reports power save {}.",
                if on { "on" } else { "off" }
            ));
        }
    }
    store.clear();
    Ok(Outcome { ok: true, message })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::tests::iface;
    use std::sync::Mutex;

    const UUID: &str = "6bf4bd07-e5d2-475c-bb29-2bec7a5da3dd";

    /// Simulated NetworkManager + driver.
    struct Fake {
        profile: Mutex<String>,
        runtime_on: Mutex<bool>,
        calls: Mutex<Vec<String>>,
        refuse_modify: bool,
        fail_up_once: Mutex<bool>,
        auth: &'static str,
        iw_supported: bool,
    }

    impl Fake {
        fn new() -> Self {
            Fake {
                profile: Mutex::new("default".into()),
                runtime_on: Mutex::new(true),
                calls: Mutex::new(vec![]),
                refuse_modify: false,
                fail_up_once: Mutex::new(false),
                auth: "yes",
                iw_supported: true,
            }
        }
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    fn ok(s: &str) -> Result<CmdOutput, CmdError> {
        Ok(CmdOutput {
            status: Some(0),
            stdout: s.into(),
            stderr: String::new(),
        })
    }
    fn fail(s: &str) -> Result<CmdOutput, CmdError> {
        Ok(CmdOutput {
            status: Some(1),
            stdout: String::new(),
            stderr: s.into(),
        })
    }

    impl Runner for Fake {
        fn run(&self, program: &str, args: &[&str], _t: Duration) -> Result<CmdOutput, CmdError> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("{program} {}", args.join(" ")));
            match (program, args) {
                ("iw", [.., "get", "power_save"]) if self.iw_supported => {
                    ok(&format!("Power save: {}\n", if *self.runtime_on.lock().unwrap() { "on" } else { "off" }))
                }
                ("iw", _) => fail("command failed: Operation not supported (-95)"),
                ("nmcli", ["-g", "UUID", "connection", "show", "--active"]) => ok(&format!("{UUID}\n")),
                ("nmcli", ["-g", _, "connection", "show", u]) if *u == UUID => {
                    ok(&format!("802-11-wireless\n\n{}\n", self.profile.lock().unwrap()))
                }
                ("nmcli", ["-g", _, "connection", "show", _]) => fail("Error: home: no such connection profile."),
                ("nmcli", ["-t", "-f", "PERMISSION,VALUE", "general", "permissions"]) => ok(&format!(
                    "org.freedesktop.NetworkManager.settings.modify.own:yes\norg.freedesktop.NetworkManager.settings.modify.system:{}\n",
                    self.auth
                )),
                ("nmcli", ["connection", "modify", _, "802-11-wireless.powersave", v]) => {
                    if self.refuse_modify {
                        return fail("Error: Failed to modify connection: Insufficient privileges.");
                    }
                    *self.profile.lock().unwrap() = v.to_string();
                    ok("")
                }
                ("nmcli", ["--wait", _, "connection", "up", _]) => {
                    let mut f = self.fail_up_once.lock().unwrap();
                    if *f {
                        *f = false;
                        return fail("Error: Connection activation failed: timeout");
                    }
                    // Global default on this fake system is "enable".
                    *self.runtime_on.lock().unwrap() = self.profile.lock().unwrap().as_str() != "disable";
                    ok("Connection successfully activated")
                }
                _ => fail("unexpected"),
            }
        }
    }

    fn store(tag: &str) -> BackupStore {
        let p =
            std::env::temp_dir().join(format!("nettune-test-{}-{tag}.json", std::process::id()));
        let _ = fs::remove_file(&p);
        BackupStore::new(p)
    }

    fn wifi() -> NetInterface {
        iface(InterfaceKind::Wifi)
    }

    #[test]
    fn validators() {
        assert!(valid_uuid(UUID));
        assert!(!valid_uuid("6bf4bd07-e5d2-475c-bb29-2bec7a5da3d;"));
        assert!(!valid_uuid("--help"));
        assert!(valid_ifname("wlo1"));
        assert!(!valid_ifname("-x"));
        assert!(!valid_ifname("a b"));
        assert_eq!(normalize_value("default").as_deref(), Some("default"));
        assert_eq!(normalize_value("2 (disable)").as_deref(), Some("disable"));
        assert_eq!(normalize_value("weird"), None);
    }

    #[test]
    fn state_supported() {
        let f = Fake::new();
        let s = store("state");
        let st = state(&wifi(), &f, &s);
        assert!(st.supported, "{:?}", st.unsupported_reason);
        assert_eq!(st.profile_value.as_deref(), Some("default"));
        assert_eq!(st.runtime_on, Some(true));
    }

    #[test]
    fn unsupported_when_driver_silent() {
        let mut f = Fake::new();
        f.iw_supported = false;
        let st = state(&wifi(), &f, &store("silent"));
        assert!(!st.supported);
        assert!(st.unsupported_reason.unwrap().contains("driver"));
    }

    #[test]
    fn unsupported_for_ethernet_and_policy_no() {
        let f = Fake::new();
        assert!(!state(&iface(InterfaceKind::Ethernet), &f, &store("eth")).supported);
        let mut f = Fake::new();
        f.auth = "no";
        assert!(!state(&wifi(), &f, &store("pol")).supported);
    }

    #[test]
    fn apply_then_revert_round_trip() {
        let f = Fake::new();
        let s = store("roundtrip");
        let o = apply(&wifi(), &f, &s, Duration::ZERO).unwrap();
        assert!(o.ok, "{}", o.message);
        assert_eq!(*f.profile.lock().unwrap(), "disable");
        assert!(!*f.runtime_on.lock().unwrap());
        let b = s.load().unwrap();
        assert_eq!(b.previous_value, "default");

        // Applying twice is refused while a backup exists.
        assert!(apply(&wifi(), &f, &s, Duration::ZERO).is_err());

        let o = revert(&f, &s, Duration::ZERO).unwrap();
        assert!(o.ok, "{}", o.message);
        assert_eq!(*f.profile.lock().unwrap(), "default");
        assert!(*f.runtime_on.lock().unwrap());
        assert!(s.load().is_none());
        assert!(revert(&f, &s, Duration::ZERO).is_err());
    }

    #[test]
    fn refused_modify_leaves_no_backup_and_no_reconnect() {
        let mut f = Fake::new();
        f.refuse_modify = true;
        let s = store("refused");
        let e = apply(&wifi(), &f, &s, Duration::ZERO).unwrap_err();
        assert!(e.contains("Insufficient privileges"));
        assert!(s.load().is_none());
        assert!(!f.calls().iter().any(|c| c.contains("connection up")));
        assert_eq!(*f.profile.lock().unwrap(), "default");
    }

    #[test]
    fn failed_activation_restores_previous_value() {
        let f = Fake::new();
        *f.fail_up_once.lock().unwrap() = true;
        let s = store("upfail");
        let e = apply(&wifi(), &f, &s, Duration::ZERO).unwrap_err();
        assert!(e.contains("restored"), "{e}");
        assert_eq!(*f.profile.lock().unwrap(), "default");
        assert!(s.load().is_none());
    }

    #[test]
    fn driver_ignoring_setting_is_reported_not_faked() {
        struct Stubborn(Fake);
        impl Runner for Stubborn {
            fn run(&self, p: &str, a: &[&str], t: Duration) -> Result<CmdOutput, CmdError> {
                let r = self.0.run(p, a, t);
                *self.0.runtime_on.lock().unwrap() = true; // driver forces power save on
                r
            }
        }
        let f = Stubborn(Fake::new());
        let s = store("stubborn");
        let o = apply(&wifi(), &f, &s, Duration::ZERO).unwrap();
        assert!(!o.ok);
        assert!(o.message.contains("still reports power save on"));
        assert!(s.load().is_some(), "backup kept so the user can revert");
        s.clear();
    }

    #[test]
    fn already_disabled_is_refused() {
        let f = Fake::new();
        *f.profile.lock().unwrap() = "disable".into();
        let e = apply(&wifi(), &f, &store("already"), Duration::ZERO).unwrap_err();
        assert!(e.contains("already disabled"));
    }

    #[test]
    fn revert_with_deleted_connection_clears_backup() {
        let f = Fake::new();
        let s = store("deleted");
        s.save(&PowerSaveBackup {
            interface: "wlo1".into(),
            connection_uuid: "00000000-0000-0000-0000-000000000000".into(),
            previous_value: "default".into(),
            applied_value: "disable".into(),
            applied_at_unix: 1,
        })
        .unwrap();
        let e = revert(&f, &s, Duration::ZERO).unwrap_err();
        assert!(e.contains("no longer exists"));
        assert!(s.load().is_none());
    }

    #[test]
    fn tampered_backup_is_ignored() {
        let s = store("tamper");
        fs::write(
            &s.path,
            r#"{"interface":"wlo1","connectionUuid":"--help","previousValue":"default","appliedValue":"disable","appliedAtUnix":1}"#,
        )
        .unwrap();
        assert!(s.load().is_none());
        s.clear();
    }
}
