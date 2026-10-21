# Karma

[中文版](README.zh-CN.md)

Karma is a local pornography-protection and digital-wellbeing application for macOS and Windows. It continuously monitors every display, closes the source application when it detects high-risk images or videos, and provides application allowlists and blocklists, internet access schedules, administrator passwords, audit logs, and protection against casual termination.

The project follows a “single product, shared core, two native executors” architecture. The UI, policies, AI, and data formats can be shared, while screen capture, process control, network filtering, background persistence, and device management must use native capabilities supported by each platform.

> On Windows, Karma hardens its two services (KarmaService and a companion watchdog) so that routine GUI tampering is denied without the Karma password: the Stop action in Services.msc / Task Manager and the "Startup type = Disabled" change are refused even to administrators (service ACL), editing the service registry keys in regedit — e.g. setting `Start = 4` to disable, or rewriting the stored service permissions — is likewise refused (registry-key ACL grants SYSTEM full control and administrators read-only), the two services restart each other every few seconds, and an End-task kill is recovered automatically by the Service Control Manager. This defeats a non-technical administrator who only uses graphical tools. It still cannot promise absolute resistance to someone who applies terminal commands (`sc.exe`), takes ownership of the service or registry key, uses the recovery environment, edits the registry hive offline, or has physical control of the device; genuine resistance to a technical administrator would require a signed kernel-level layer (PPL/ELAM), which this unsigned test build does not include. Running the monitored person as a standard user (with the administrator password on a separate account) remains the strongest option and makes even the technical vectors inaccessible.

## Windows Test Installer

New users can download the single-file `Karma-windows-x64-test-v0.3.0-setup.exe` from the [v0.3.0 Windows Test Build](https://github.com/csbuddhist/Karma/releases/tag/v0.3.0). On Windows 10 22H2 or Windows 11 x64, run it as an administrator to install the files, verify their hashes, register the Service, and create shortcuts. The installer includes the official Microsoft Visual C++ 2015–2022 x64 Redistributable: it verifies the Microsoft signature and installs it silently only when the runtime is missing or older than 14.44, before registering the Service.

`main` also retains the auditable **cloneable Windows test bundle** at [`release/windows-x64-test/`](release/windows-x64-test/). From v0.2.4, setup upgrades an existing v0.2.4+ installation in place with the administrator password: it submits a signed bundle to the running service, which accepts only a strictly newer version, validates private staged copies, replaces files with rollback, and restarts the services. Protection services remain registered. Only v0.2.3 and earlier use the legacy password-authorized uninstall/reinstall flow. The console starts in the background at sign-in by default; this can be disabled in System Settings. EXEs and setup still lack Authenticode signatures; the Ed25519 release signature authenticates update contents, not the Windows publisher. See the [installation guide](docs/windows-installation-guide.md) and [pending Windows update acceptance checklist](docs/windows-update-acceptance.md).

The optional **super password** should be set and held by a supervisor, separately from the administrator password. The service compares every policy change and requires the super password for weakening protection: disabling protection or recognition, lowering sensitivity, relaxing website/application/keyword/schedule/repeat-closure rules, forgetting a remembered title, or restoring the network early. Tightening protection uses the administrator session. Initial setup rechecks the administrator password; the super password must have at least 10 characters and differ from it. Changing it requires the current super password. Without a super password, existing behavior is preserved.

Uninstall immediately with the super password, or request uninstall in the console with the administrator password, wait **72 hours**, then uninstall with the administrator password during the following **24 hours**. Requests survive restarts, can be cancelled, and are audited; expired requests must be renewed. This is also the recovery route for a forgotten super password. Software updates need only the administrator password. `updates.autoInstall` defaults to enabled; the service checks GitHub Releases periodically, and the console can check immediately. All updates require the embedded developer key's Ed25519 signature, SHA-256 file hashes, and a strictly higher version. Same-version reinstall and downgrade are refused. A technical Windows administrator can still take ownership of files, use safe mode, or otherwise bypass user-mode protection; keeping the Windows administrator account with the supervisor and using a standard account daily remains the strongest supplement.

## Administrator and Super Password Permissions (v0.2.4)

**Summary:** Karma uses two password levels: an ordinary administrator password and a super password. After a super password is set, the ordinary administrator can still do daily management, tighten rules, change the ordinary password, manage evidence, upgrade the software, and request or cancel uninstall. Viewing an original evidence image, setting the super password for the first time, and requesting uninstall each ask for the ordinary password again. Weakening protection or uninstalling immediately needs the super password.

The following describes the current **installed Windows Service**, with a super password already configured. Entering the ordinary administrator password unlocks the console and creates an authenticated session. Most everyday actions use that session; the actions marked below require entering the ordinary password again. The super password authorizes sensitive actions and immediate uninstall; console login still uses the ordinary password.

| Area | Ordinary administrator password or session | Additional super password required |
| --- | --- | --- |
| Console and status | Unlock or lock the console; view Service/Agent/display health, policy, network state, current application bans, update status, and uninstall countdown; search and page through remembered titles, with the recognition reason for each; review titles marked as misjudged and let one be remembered again | Forgetting a remembered title, or marking it as misjudged |
| Ordinary administrator password | Change it after entering the current ordinary password again; the new password must have at least 10 characters and differ from the configured super password | No super password needed for this change |
| Super password setup | Set it for the first time after entering the ordinary password again; it must have at least 10 characters and differ from the ordinary password. The supervisor should enter and keep it | Changing an existing super password requires the current super password; the ordinary password cannot overwrite or clear it |
| Protection and recognition | Enable protection, image recognition, OCR, title matching, or remembered titles; increase sensitivity by lowering the enforcement threshold | Disable any of those switches, or reduce sensitivity by raising the threshold |
| Website rules | Add or enable block rules; remove or disable allow rules | Add or enable allow rules; remove, disable, or narrow block rules |
| Application rules | Add or enable “Always block” rules; remove or disable “Allow” rules | Add or enable “Allow” rules; remove, disable, or narrow “Always block” rules |
| Keywords | Add or enable enforced keywords; add keywords to the built-in list; turn on the built-in terms prone to false positives; remove or disable exemptions | Remove, disable, or replace enforced custom keywords; remove a keyword added to the built-in list; turn off the terms prone to false positives; add or enable exemptions |
| Network schedules | Add, enable, or extend restricted periods; expand restricted targets while preserving all existing coverage | Remove, disable, shorten, move, or retarget periods in a way that reduces existing restricted coverage; restore network access before a block ends |
| Repeat-closure limits | Lengthen the counting window or reduce the number of closures needed to trigger a ban | Shorten the counting window or increase the number of closures needed |
| Currently restricted applications | View each restricted application and when its restriction ends | **Always allow**: one click sets that application to “Allow” in the application list (an “Always block” rule matching it also becomes “Allow”) and lifts its current restriction immediately. It is saved at once, separately from other unsaved edits, and audited |
| Event evidence | View and filter the evidence list, with the reason each item was saved; enter the ordinary password again to view each original; delete one item or several selected items after confirming; enable or disable evidence saving | These evidence operations do not require the super password; forgetting a remembered title is a separate protected operation |
| Policy backups | Export policy, import a backup for review, and save it when it preserves or strengthens protection | Saving an imported policy that weakens protection; importing does not bypass service authorization |
| Console preferences | Change the interface language or whether the console starts at sign-in. Closing the console hides its window while protection continues | These preferences do not require the super password |
| Software updates | Enable or disable automatic installation; check and install updates now; enter the ordinary password in setup to request an in-place upgrade | No super password needed. Every upgrade still requires a valid developer signature, matching file hashes, and a strictly higher version |
| Uninstall request | Enter the ordinary password again to request uninstall; cancel a request with the administrator session; after the 72-hour wait, enter the ordinary password to uninstall during the following 24 hours | Immediate uninstall before the waiting period has elapsed requires the super password |

The table describes authorization, not a guarantee that an operation can succeed regardless of state: updates and uninstall cannot execute concurrently, and session, validation, revision, and service-availability checks still apply. Policy changes take effect only after saving. The Service compares the **entire submitted policy** with the current one: if any part weakens protection, the whole save requires the super password, even when other parts strengthen it. Renaming or reorganizing rules is allowed without it only when the Service's comparison finds no weakening. Direct service requests are subject to the same checks.

An uninstall request remains valid across reboots. Its administrator-only window is **72 hours after the request up to, but not including, 96 hours after the request**. Cancelling or expiry removes that authorization; a new request starts a new 72-hour wait. Requests, readiness, cancellation, expiry, and uninstall approval are audited. A forgotten super password uses this delayed uninstall route; there is no separate super-password reset or one-time recovery-code feature. The ordinary password alone cannot immediately disable protection or reset an existing super password.

Uninstall keeps local data by default, including the super-password hash; reinstalling with that data retained does not reset the super password. If recovery requires a fresh setup, use `Uninstall-Karma.ps1 -PurgeData` after uninstall is authorized, then reinstall and enroll again. Purging also permanently deletes the ordinary password, policy, audit records, bans, remembered titles, keys, and encrypted evidence.

**When no super password is configured**, the ordinary administrator session can also save weakening policies and forget remembered titles; early network restore and immediate uninstall recheck the ordinary password. The 72-hour uninstall wait is not required. Signed-update and version restrictions still apply.

The current console does **not** expose a complete persisted audit-log reader. The service status interface provides the audit-record count. Deleting an evidence image does not remove a remembered title or relax the enforcement rules.

These are Karma application permissions. UAC/Windows administrator rights are still required where the installer or uninstaller requests elevation. A technical Windows administrator can bypass user-mode protection through ownership changes, safe mode, or offline edits; using a standard Windows account with a supervisor-held administrator account remains recommended.

## Design Principles

- Local first: screen images are processed in memory by default and are neither uploaded nor saved in their original form.
- Timely enforcement: continuous-frame detection replaces infrequent screenshots, with a target response time of 1–3 seconds after high-risk content appears.
- Independent multi-display processing: each display is captured, inferred, and tracked independently.
- Least privilege: the management UI holds no system privileges; privileged operations may be performed only by a signed service.
- No HTTPS decryption: no self-signed root certificate is installed, and no global HTTPS man-in-the-middle proxy is used.
- Layered protection: ordinary password protection, system-service self-recovery, and device management provide progressively stronger enforcement.
- Auditable: every policy change and enforcement action produces a structured event, without recording sensitive screen content or complete browsing activity.

## Overall Architecture

```text
Tauri + TypeScript management UI
              │ authenticated local IPC
              ▼
          Shared Rust core
  ┌─────────────────────────────┐
  │ ONNX pornography detection  │
  │ Time and app policy engine  │
  │ Continuous-frame state      │
  │ machine                     │
  │ SQLite, encryption, audit   │
  │ logs                        │
  │ Configuration validation    │
  │ and unified IPC protocol    │
  └─────────────────────────────┘
         │                 │
         ▼                 ▼
 macOS session agent    Windows session agent
 ScreenCaptureKit       Windows.Graphics.Capture
 NSWorkspace            Win32 / WinRT
         │                 │
         ▼                 ▼
 macOS privileged       Windows privileged
 executor               service
 LaunchDaemon           Windows Service
 Endpoint Security      WFP / App Control
 Network Extension      Optional signed driver
         │                 │
         └──── MDM / device policy ────┘
```

The management UI, session agent, and privileged executor must run as separate processes. The system service runs in a privileged context, but screen capture can occur only in the user's graphical session. A Windows Session 0 service or macOS LaunchDaemon must not be treated as a screen-capture process.

## Process Topology

### Windows

```text
KarmaService.exe                 LocalSystem, starts automatically at boot
  ├─ Policy enforcement and watchdog
  ├─ WFP/firewall rule management
  ├─ Application enforcement and service recovery
  └─ Starts and validates, for every signed-in session:
       KarmaAgent.exe            Current user session
         ├─ Multi-display capture
         ├─ Foreground window/PID attribution
         ├─ ONNX inference
         └─ Communication with the UI and Service

KarmaUI.exe                      Tauri management UI, started on demand
```

### macOS

```text
com.karma.daemon                 LaunchDaemon, root
  ├─ Policy enforcement and watchdog
  ├─ Network Extension management
  ├─ System Extension coordination
  └─ Validates the signed-in session agent

KarmaAgent.app                   Current graphical user session
  ├─ ScreenCaptureKit multi-display capture
  ├─ Foreground application attribution
  ├─ ONNX inference
  └─ Management UI

KarmaEndpointExtension           Endpoint Security System Extension
```

## Shared Core

The shared core is written in Rust and produces native libraries for Windows and macOS. It can also be loaded by the session agent as a separate process.

### Policy Engine

The unified engine supports:

- Allowed and blocked time ranges for all seven days of the week at 15-minute granularity.
- Browser, media player, game, and custom application categories.
- Rules based on application path, bundle ID, publisher signature, and file hash.
- Blocklists, allowlists, and a default policy.
- Browser-domain allowlists and blocklists. An allowlisted domain always wins; a blocklisted
  domain closes immediately without waiting for an image-risk threshold.
- Foreground-window title matching with a versioned explicit-content list of 2,543 terms in 26
  languages (`assets/keyword-lists/`). Each term has a category (sexual activity or body parts),
  a match mode (whole word, contains, or word prefix), and a confidence: the 1,592 strict terms are
  always active, while the 951 terms that also appear in ordinary titles (such as “奶” or
  “adult”) are off by default and can be turned on. Administrators can add their own terms to the
  built-in list with a language, category, and match mode; added terms are always active. Console
  custom high-risk and sensitive keywords also join matching, and exemption keywords suppress
  matches. The console keyword page filters the built-in list by language, category, and
  confidence, and supports adding, editing, enabling, deleting, and searching custom keywords.
- Remembered window titles: when explicit image detection closes an application, the Service
  remembers that window's title (the page title in browsers) and canonical host; the same
  application showing the same title again is closed on sight. The console's Content
  Recognition page lists remembered titles with why each was remembered (image risk score and the threshold at the time; titles from v0.2.6 or earlier show "not recorded"), deletes them, and can switch the behavior off. A misjudged title can be marked **Misjudged**: it is forgotten and never remembered again, though a window closed by a later misjudgment is still closed. Misjudged titles are listed separately, encrypted like remembered titles, and can be remembered again.
- Temporary allowances, remaining usage time, and cooldown periods.
- Corrections for screen locking, sleep, time-zone changes, and daylight saving time.
- Policy priority and conflict explanations.

Recommended priority:

```text
Device-enforced policy > Emergency-disable policy > Schedule block > App blocklist
                       > Temporary allowance > App allowlist > Allow by default
```

For browser content enforcement, the more specific precedence is `website allowlist > website
blocklist > application allow > title keyword > image threshold`. Domain rules include subdomains
but not sibling domains. Console-managed application rules are enforced by the Service: an
"allow" application (matched by executable name or full path tail) is never closed by image or
title enforcement — except on a blocklisted site — and an "always block" application is closed
whenever its foreground window is observed.

Policy evaluation returns a structured result rather than directly performing system operations:

```rust
Decision {
    action: Allow | Warn | CloseGracefully | Terminate | BlockNetwork,
    reason: ReasonCode,
    policy_id: String,
    expires_at: Option<Timestamp>,
}
```

### AI Detection

- ONNX Runtime with local CPU/GPU inference.
- The same model, labels, normalization parameters, and thresholds are used on both platforms.
- Each display maintains an independent sliding window.
- Bounded frame preparation is capped at 4 frames per second and image inference at 2 frames per second, avoiding unbounded capture-callback work while keeping preparation admission latency within 250 ms.
- Before inference, images are resized and very small regions are blurred to reduce false positives from text and avatars.
- Video and still images use the same continuous-frame state machine.

Suggested decision rules:

```text
score >= 0.95                               → Enforce immediately
At least 3 frames with score >= 0.82        → Enforce
within the last 5 seconds
At least 5 frames with score >= 0.70        → Warn or enforce
within the last 8 seconds
Below threshold continuously for 10 seconds → Clear risk state
```

Thresholds must be updatable through signed configuration. After a model match, first identify the foreground window on that display and its owning PID/Bundle ID, and then act on the source application to avoid terminating unrelated browsers or background processes.

In the current Windows test build, saving the console sensitivity also updates the immediate-enforcement threshold, including migration of previously saved mismatched values. Disabling image recognition stops image classifier work and prevents image evidence or disposition events.

### Data and Encryption

- SQLite uses WAL mode.
- Configuration, policies, and audit logs use versioned schemas.
- Administrator passwords are stored only as Argon2id hashes and must never be stored reversibly.
- Database keys are wrapped by Windows DPAPI or macOS Keychain.
- IPC keys are generated during initial installation and stored in the system credential store.
- Update packages, models, and policies are all verified with Ed25519 signatures.

By default, only the following are recorded:

- Time, device, user, and display number.
- Application identifier, publisher, and enforcement result.
- Model version, risk level, and normalized score.
- Policy ID, reason code, and component health status.

By default, screen captures, window contents, full URLs, user input, and dynamically generated site certificates are not recorded. The Windows Agent processes the active window title and browser address transiently in memory, sends only the canonical host plus a bounded title to the local Service for independent policy validation, and never persists either value in audit or evidence records. The one exception is the remembered-title feature (on by default): after explicit image detection successfully closes an application, the Service stores that window's title, canonical host, and executable path so it can close the same page again. They are encrypted with AES-256-GCM, under a key derived from the DPAPI-protected evidence key, inside the Service's ACL-protected SQLite database (`C:\ProgramData\Karma\service-state.db`). At most 5,000 titles are kept, oldest first out. The Agent receives only 128-bit digests, never the titles themselves. The administrator can search and review them in the console; deleting a title or disabling the feature requires the super password when configured. Titles never enter the audit log.

## Screen Capture

### Windows

- Use `EnumDisplayMonitors` to enumerate all active displays.
- Use `IGraphicsCaptureItemInterop::CreateForMonitor` to create a `Windows.Graphics.Capture` session for each `HMONITOR`.
- Run one `KarmaAgent` per signed-in session and listen for display connection, disconnection, resolution, scaling, rotation, and HDR changes.
- Use D3D11 textures for GPU scaling and avoid full-size GPU-to-CPU copies whenever possible.
- If WGC is unavailable, fall back to DXGI Desktop Duplication, but record the fallback event.
- Capture is not guaranteed on the lock screen, the UAC secure desktop, or DRM-protected content.

The current repository implements Windows frame input, image-inference and OCR-inference slicing, foreground-window title observation, browser address-bar host discovery, as well as the Windows Service, authenticated IPC, policy persistence, Agent watchdog, health heartbeat, identity-bound enforcement executor, and a DPAPI + AES-GCM evidence store. The Agent polls the active window in a dedicated apartment, evaluates context locally, and sends a bounded matching observation to the Service; the Service independently reapplies allowlist, blocklist, and multilingual title policy before authorizing a process-identity-bound close. Image observations also carry only the canonical browser host so an allowlisted site cannot be closed by a high image score. When “Save event evidence” is enabled, only image-inference frames that reach the immediate-enforcement threshold are encoded by the Agent and submitted to the Service for encrypted storage; the Service revalidates both policy and threshold. The Agent loads image and OCR models from a verified local manifest and does not emit recognized text, scores, titles, URLs, or screen content to logs. Continuous-frame risk fusion is still not connected.

Service state (administrator password hash, policy, up to 50,000 audit entries, evidence metadata, repeat-offender bans, and remembered titles) lives in a transactional SQLite database (WAL, `synchronous=FULL`, `secure_delete`). Every change commits atomically, so a crash or power loss cannot leave a half-written state file. Builds before v0.2.3 stored a JSON file; it is imported automatically on first start, in the same transaction that initializes the database, and kept as `service-state.json.migrated`. Downgrading to an earlier build is not supported: it would no longer find its JSON file and would start with first-time setup.

Browser host discovery currently supports Chrome, Edge, Firefox, Brave, Opera/Opera GX,
Vivaldi, and Arc on Windows through UI Automation. Rules are domain-level (including subdomains),
not path-level. If a browser hides or does not expose its address bar to accessibility, Karma cannot
identify an allowlisted or blocklisted host for that window; title and image rules continue to work.

The management UI is located at [`apps/karma-ui/`](apps/karma-ui/). Windows builds connect to `KarmaService` through a native named pipe that supports concurrent clients. Administrator passwords, sessions, live Agent/display status, policy revisions, and evidence viewing are all controlled by the Service. The Event Evidence page shows each item's local capture date and time, newest first, and can be filtered by a start/end date-time range. System Settings can change the administrator password after re-verifying the current one, and can export the current policy to a JSON backup file or import a previously exported backup for review and restore; import validates the backup schema and policy before anything reaches the console. Service sessions are held in memory only: if `KarmaService` is stopped and started again, the console detects the invalid session and returns to the unlock screen, and unlocking once more restores the connection instead of leaving a permanent "service not connected" status. If the connection fails, the GUI shows an explicit Service connection error instead of presenting an unknown authentication state as a password-unlock screen. Non-Windows development builds continue to use an isolated local backend.

Tests on a macOS development machine and cross-compilation for `x86_64-pc-windows-msvc` prove only that the portable algorithms, Rust type constraints, and Windows API signatures are correct. GPU driver behavior, actual frame colors, resource cleanup, and multi-display performance must be verified on Windows 10 22H2/Windows 11 according to the [Windows Frame Pipeline Hardware Acceptance Checklist](docs/windows-frame-pipeline-acceptance.md) and [Windows ONNX Hardware Acceptance Checklist](docs/windows-onnx-acceptance.md). Runtime acceptance is not considered complete until this evidence has been recorded.

### macOS

- Use `SCShareableContent` to enumerate `SCDisplay` instances.
- Create an independent `SCStream` for each display, with output sent to a serial capture queue.
- Listen for display-configuration changes and rebuild the corresponding stream.
- Explicitly request Screen Recording permission on first launch. If the permission is revoked, enter a fail-safe state and notify the guardian.
- Do not attempt to bypass TCC, system capture indicators, or protected-content restrictions.

### Frame Attribution

The capture agent keeps the following information synchronized:

- Current foreground window and PID.
- Intersection area between each window and display.
- Full-screen applications and picture-in-picture windows.
- Multi-window browser scenarios.

A matched frame is attributed preferentially to the foreground window covering the largest area of that display. When reliable attribution is impossible, show a warning and apply a temporary overlay instead of directly terminating multiple candidate processes.

## Application Control

Application control borrows the idea of “continuous process monitoring, immediate termination, and service-driven restart” from Zhan Chi Niao, while using supported, signable, and auditable system interfaces.

### Enforcement Escalation

```text
1. Block new network connections
2. Send a graceful-close request to the window
3. Wait 2 seconds and check again
4. Terminate the confirmed target process
5. Prevent the same application from restarting during the cooldown period
6. Record the event and notify the management interface
```

After a browser match, the corresponding browser process group is closed by default. For browsers with multiple profiles, the implementation should identify the process owning the window whenever possible. Media players, image viewers, and games can be handled directly by their main processes.

Repeat-offender cooldown (implemented on Windows): once the same executable is closed 5 times within 20 minutes (both configurable on the console's Application Control page: 1–1440 minutes and 2–20 closures), it is banned for one hour. Every successful closure counts, whether triggered by images, titles, URL rules, or remembered titles. During the ban, any foreground observation of that executable is terminated immediately with no grace period, superseding website allow rules and application allow rules. The Service publishes active bans and remembered titles to the Agent, which refreshes them right after each closure and with every heartbeat (about 10 seconds), so a relaunched program is closed even when its window shows nothing objectionable. Enforcement kills during a ban do not extend it. The ban is persisted in the service state, survives service restarts and reboots, is listed on the Application Control page, and is recorded in the audit log as `repeat_offender_banned`; turning protection off (`protectionEnabled`) suspends enforcement.

### Windows Application Control

Basic mode:

- Monitor new processes using ETW/WMI/Win32 process events.
- Verify the executable path, signing publisher, and parent process.
- Use `WM_CLOSE` for graceful shutdown.
- After a timeout, `KarmaService` uses a restricted handle to call `TerminateProcess`.
- The service maintains a cooldown table and acts again if an application restarts.

Hardened mode:

- Use AppLocker or WDAC to enforce application-control policies during blocked periods.
- Use the WFP ALE layer to block network connections by application identity.
- If basic mode cannot meet self-protection goals, develop a minimal EV-signed driver. The driver is responsible only for process-handle protection and event notification; it must not contain AI, configuration, or UI logic.
- Global DLL injection and unrestricted keyboard hooks are prohibited.

### macOS Application Control

Basic mode:

- Use `NSWorkspace` to obtain running applications and launch/termination notifications.
- Use `NSRunningApplication.terminate()` for graceful shutdown.
- After a timeout, the privileged executor sends a controlled termination signal.

Hardened mode:

- Subscribe to Endpoint Security `AUTH_EXEC` events and allow or deny execution according to locally cached policies before the process actually runs.
- Subscribe to relevant signal events and audit attempts to terminate Karma components.
- Authorization callbacks must not access SQLite, the network, or ONNX. All evaluable policies are precompiled into read-only in-memory snapshots to ensure a response before the system deadline.
- If Apple has not approved the Endpoint Security entitlement, the product must degrade gracefully and clearly display the capability difference.

## Exit Prevention and Self-Protection

### Level 1: Password Protection

- The administrator session controls routine settings; weakening protection requires the configured super password. Uninstall uses super-password authorization or the delayed administrator window described above.
- Closing the UI only hides the management interface; it does not stop the Agent or Service.
- Consecutive failures trigger exponential backoff and an audit event.
- A forgotten super password uses the audited 72-hour delayed uninstall route described above. Separate one-time recovery codes are not implemented.

### Level 2: Service Protection and Automatic Recovery

- Windows Service/LaunchDaemon starts automatically at boot.
- Service and Agent exchange bidirectional heartbeats, and the system service recovers either component after an abnormal exit.
- The service accepts commands only from signed clients over authenticated local IPC.
- The installation directory is owned by `SYSTEM/root` and administrators, is read-only to ordinary users, and must not use permissions such as `Everyone: FullControl`.
- The service-control ACL does not grant ordinary users permission to stop, modify, or delete the service.
- Component signatures and hashes are verified at startup. On failure, the system enters a fail-safe state and notifies the management interface.
- Native operating-system recovery mechanisms are used instead of having two user-space processes restart each other indefinitely.

### Level 3: Device Management

Windows:

- Standard users do not possess administrator credentials.
- MDM deploys the service, WFP, WDAC/AppLocker, and uninstall restrictions.
- BitLocker, tamper-resistant boot policies, and Secure Boot remain enabled.

macOS:

- Standard users do not possess administrator credentials.
- MDM deploys PPPC, System Extension, Network Extension, and non-removable application policies.
- FileVault and System Integrity Protection remain enabled.

### Explicit Boundaries

The following situations can only be detected or reported afterward; prevention cannot be guaranteed:

- An administrator/root user intentionally disables or removes components.
- Safe Mode, recovery environments, or offline disk modification.
- The user revokes macOS Screen Recording permission.
- Secure Boot/SIP is disabled, the operating system is reinstalled, or the startup disk is replaced.
- Physical obstruction, external capture devices, or content played on another device.

## Network Schedule Control

Network control governs only which applications may access the network and when. Pornographic-content detection remains the responsibility of the on-screen AI.

Current status: the console's Network Schedules page configures any number of time blocks for each day from Monday to Sunday (blocks may cross midnight) and one of three targets per rule: browsers (common browsers plus applications categorized as browsers), selected applications, or all networked applications. Single-range rules saved by earlier builds are migrated automatically. `KarmaService` validates every schedule, evaluates it in the device's local time zone, reports whether a block is active and when it ends, and records the start and end of each restricted period in the audit log. During a block, network access can be restored early only from the console by entering the super password again (the administrator password if none is configured); the restore lasts until the current block ends, is persisted across Service restarts, and is audited. On Windows, the service now applies `ServiceCore::network_enforcement` through persistent WFP filters immediately on startup and every 60 seconds afterward. Schedule changes, policy changes, and early restore take effect at the next poll (up to about 60 seconds). Windows hardware acceptance is pending; see [the network enforcement checklist](docs/windows-network-enforcement-acceptance.md).

### Windows

- Persistent WFP filters cover IPv4/IPv6 ALE connect and receive/accept, without restricting the protocol. Browser and selected-application rules match executable names, trailing path components, or full application identities, including programs launched after the rules are installed. These rules apply across users.
- Each update atomically replaces only Karma-owned filters. Failures retain the previous set, retry at the next 60-second poll, and record failure/recovery transitions in the audit log.
- All-network blocking preserves exact Karma installation paths and core Windows network infrastructure (`System`, `ntoskrnl.exe`, `svchost.exe`, `lsass.exe`, `services.exe`, and `wininit.exe` under the system directory). It does not exempt arbitrary applications by name or the whole Windows directory.
- A service crash retains the last committed rules. Startup reconciles them against the current schedule; normal stop and password-authorized uninstall remove them. An elevated `KarmaService.exe --clear-network-filters` command removes only Karma WFP objects for recovery. Do not run recovery while the service is enforcing a schedule.
- New connections and existing ALE flows are subject to WFP authorization/reauthorization. Hardware testing must verify TCP, UDP/QUIC, IPv6, existing streams, and VPN/proxy coexistence; proxy/service-hosted traffic can inherit the intermediary process identity.

### macOS

- Use a Network Extension Content Filter or DNS Proxy.
- Block connections by application and policy; DNS is used only for domain-level categorization.
- Do not enforce control by editing `/etc/hosts` or repeatedly switching the system proxy.

A browser extension can serve as an optional enhancement for full-URL categorization and friendly block pages, but it cannot be the only security boundary.

## IPC and Privilege Boundaries

The unified IPC protocol uses length-prefixed messages and a version field:

```text
UI       → Core/Service: Read status; submit authenticated policy changes
Agent    → Core        : Frame-inference requests and window-attribution data
Core     → Service     : Structured Decision; never an arbitrary command
Service  → Agent       : Health checks, capture configuration, and policy snapshot version
```

Security requirements:

- Windows uses named pipes with explicit DACLs.
- macOS uses XPC and verifies the Team ID, Bundle ID, and code-signing requirement.
- The server does not accept generic commands such as “execute an arbitrary path” or “terminate an arbitrary PID.”
- Every PID operation must also verify the process start time, signature, and application identity to prevent PID-reuse attacks.
- IPC includes a nonce, timestamp, and session key, and rejects replays.

## Fail-Safe Behavior

| Failure | Default behavior |
|---|---|
| AI model fails to load | Stop pornography detection, retain time and application policies, and continue alerting |
| Capture fails on one display | Rebuild that display's stream without affecting other displays |
| Agent crashes | Service restarts the Agent; repeated failures in a short period trigger rate-limited recovery |
| Service crashes | Operating-system service recovery restarts it; the network retains the last policy |
| Policy database is corrupted | Use the latest signed snapshot and enter read-only mode |
| System time changes abruptly | Verify with a monotonic clock and recalculate time policies |
| IPC authentication fails | Reject the request and record a security event |
| Permission is revoked | Display a non-dismissible alert; notify the management interface on managed devices |

## Installation, Signing, and Updates

### Windows

- Use MSI to install the Service, Agent, UI, and optional driver.
- Sign every EXE, DLL, MSI, and driver with a trusted code-signing certificate.
- Configure the service SID, directory ACLs, named-pipe DACLs, and recovery policy during installation.
- Uninstallation requires UAC and super-password or delayed administrator authorization; in MDM mode, the device policy determines whether removal is permitted.

### macOS

- Distribute signed `.app`/`.pkg` packages and complete Apple notarization.
- On first run, guide the user through Screen Recording, System Extension, and Network Extension permissions.
- In MDM environments, preapprove allowed capabilities through PPPC and System Extension payloads.
- The updater verifies the Team ID, designated requirement, and update-package signature.

The update process uses A/B component directories: download, verify the signature, run preflight checks, switch versions, and confirm health. Automatically roll back on failure. Database migration must support forward recovery and must never erase configuration after a failed update.

## Tradeoffs Compared with Zhan Chi Niao

Ideas retained:

- Separate the system service from the signed-in session agent.
- Monitor processes in real time and act when a policy matches.
- Let the service recover core components.
- Run model inference locally instead of uploading screen content to the cloud.
- Evaluate schedules and application allowlists/blocklists through one policy system.

Explicitly not adopted:

- Self-signed root certificates and a global HTTPS man-in-the-middle proxy.
- Saving complete screens unencrypted to disk every few minutes.
- Unsigned core programs and installation directories writable by ordinary users.
- Global DLL injection, hidden files, and excessive keyboard hooks.
- Coupling passwords, AI, networking, UI, and self-protection into a monolithic process.

## Technology Stack

| Layer | Technology |
|---|---|
| UI | Tauri + TypeScript |
| Shared core | Rust |
| AI | ONNX Runtime |
| Data | SQLite + DPAPI/Keychain |
| macOS integration | Swift, ScreenCaptureKit, XPC |
| macOS hardening | Endpoint Security, Network Extension, MDM |
| Windows integration | Rust/C++, Win32, WinRT, WGC |
| Windows hardening | Windows Service, WFP, WDAC/AppLocker, optional WDK driver |

C# can be used to quickly validate WGC or develop management tools, but the production Windows executor should prefer Rust/C++ to avoid an additional runtime and cross-language service boundaries. WFP callouts and kernel self-protection must also use native implementations supported by the WDK.

## Implementation Phases

### Phase 1: Verifiable MVP

- Rust policy engine, SQLite, and IPC schema.
- Windows/macOS single-display capture adapters.
- ONNX inference and continuous-frame evaluation.
- Foreground-application attribution and graceful shutdown.
- Tauri settings interface and administrator password.

### Phase 2: Multi-Display Support and Basic Persistence

- Multi-display hot-plug handling and independent state machines.
- Windows Service and macOS LaunchDaemon.
- Agent heartbeat, self-recovery, and signature verification.
- Application allowlists/blocklists, schedules, and cooldown policies.
- In-memory inference and privacy auditing.

### Phase 3: Network and Hardened Control

- Windows WFP/ALE filtering.
- macOS Network Extension.
- Endpoint Security `AUTH_EXEC`.
- WDAC/AppLocker and MDM configuration templates.
- Tests for permission revocation, offline operation, and fail-safe behavior.

### Phase 4: Production Release

- Complete Windows/macOS signing, notarization, and installers.
- Secure model and application updates with A/B rollback.
- Performance, false-positive rate, battery, and multi-user session testing.
- Accessibility, privacy notice, data export, and uninstall flow.

## Acceptance Criteria

- Every active display is captured independently, and monitoring resumes within 5 seconds after a display is connected or disconnected.
- Typical pornographic video triggers enforcement within 3 seconds of appearing, and the continuous-frame false-positive rate meets the test-set target.
- During blocked periods, an application is intercepted or handled within 500 milliseconds of launch.
- Ordinary users cannot stop the system service, change policies, or uninstall components.
- The Agent recovers within 5 seconds after an abnormal exit without entering an infinite restart storm.
- Average idle CPU usage remains below 3%; tiered budgets for active AI processing are set according to the hardware.
- Default operation for 30 days produces no raw screen files.
- No root CA is installed, and complete webpage content and user input are not recorded.
- Every management action and system enforcement event has a verifiable audit record.

## Testing Priorities

- Two and three displays, mixed DPI, rotation, HDR, sleep/wake, and Remote Desktop.
- Browser video, image viewers, media players, games, and picture-in-picture.
- Multiple signed-in users, fast user switching, screen locking, and session sign-out.
- Repeated application restarts, process-tree changes, PID reuse, and path changes after updates.
- Agent/Service termination, database corruption, model corruption, and insufficient disk space.
- Revoked macOS TCC permissions, unapproved System Extensions, and Endpoint Security timeouts.
- Coexistence of Windows WFP with VPNs, proxies, security software, and enterprise firewalls.
- False positives, false negatives, skin-tone bias, animated content, and medical and artistic contexts.
- Three threat levels: ordinary user, administrator, and MDM-managed device.

## Compliance and Transparency

Karma should be used only for content protection by device owners, guardians, or organizations acting within the scope of lawful authorization. Installation must clearly disclose screen monitoring, application enforcement, logging scope, and the uninstall process. The product must not conceal the fact that monitoring occurs, collect keyboard input, use screenshots for training, or use technical means to bypass operating-system privacy prompts or device-owner permissions.

## Protection control

The master switch at the top of Protection Overview stops or resumes protection. Toggle it, then click **Save settings** to apply the change; System Settings exposes the same switch. When a super password is configured, stopping protection requires it; resuming uses the administrator session.

## Repository and License

The canonical repository is [csbuddhist/Karma](https://github.com/csbuddhist/Karma). Use this repository for future pulls, pushes, and releases.

Karma source code is licensed under the GNU General Public License version 3 only (`GPL-3.0-only`); see [LICENSE](LICENSE). Third-party dependencies and model assets retain their respective licenses.
