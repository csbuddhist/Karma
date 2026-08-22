# Karma

[中文版](README.zh-CN.md)

Karma is a local pornography-protection and digital-wellbeing application for macOS and Windows. It continuously monitors every display, closes the source application when it detects high-risk images or videos, and provides application allowlists and blocklists, internet access schedules, administrator passwords, audit logs, and protection against casual termination.

The project follows a “single product, shared core, two native executors” architecture. The UI, policies, AI, and data formats can be shared, while screen capture, process control, network filtering, background persistence, and device management must use native capabilities supported by each platform.

> This software cannot promise absolute resistance to removal by anyone with local administrator or root access, access to the recovery environment, or physical control of the device. Strong enforcement mode must be combined with standard user accounts and device-management capabilities such as MDM and WDAC/AppLocker.

## Windows Test Installer

New users can download the single-file `Karma-windows-x64-test-v0.1.3-setup.exe` from the [v0.1.3 Windows Test Build](https://github.com/DrWeiZhou/Karma/releases/tag/v0.1.3). On Windows 10 22H2 or Windows 11 x64, run it as an administrator to install the files, verify their hashes, register the Service, and create shortcuts. The Microsoft Visual C++ 2015–2022 x64 Redistributable is still required.

`main` also retains the auditable **cloneable Windows test bundle** at [`release/windows-x64-test/`](release/windows-x64-test/) for diagnostics and reproducible packaging. The installer remains unsigned development/test software, and uninstall still requires the Karma administrator password; it is not a signed production installer. See the [Windows Installation and Testing Guide](docs/windows-installation-guide.md) for details.

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
- Temporary allowances, remaining usage time, and cooldown periods.
- Corrections for screen locking, sleep, time-zone changes, and daylight saving time.
- Policy priority and conflict explanations.

Recommended priority:

```text
Device-enforced policy > Emergency-disable policy > Schedule block > App blocklist
                       > Temporary allowance > App allowlist > Allow by default
```

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
- The default sampling rate is 2 frames per second, dropping no lower than 1 frame per second when the system is busy.
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

By default, screen captures, window contents, full URLs, user input, and dynamically generated site certificates are not recorded.

## Screen Capture

### Windows

- Use `EnumDisplayMonitors` to enumerate all active displays.
- Use `IGraphicsCaptureItemInterop::CreateForMonitor` to create a `Windows.Graphics.Capture` session for each `HMONITOR`.
- Run one `KarmaAgent` per signed-in session and listen for display connection, disconnection, resolution, scaling, rotation, and HDR changes.
- Use D3D11 textures for GPU scaling and avoid full-size GPU-to-CPU copies whenever possible.
- If WGC is unavailable, fall back to DXGI Desktop Duplication, but record the fallback event.
- Capture is not guaranteed on the lock screen, the UAC secure desktop, or DRM-protected content.

The current repository implements Windows frame input, image-inference and OCR-inference slicing, as well as the Windows Service, authenticated IPC, policy persistence, Agent watchdog, health heartbeat, identity-bound enforcement executor, and a DPAPI + AES-GCM evidence store. When “Save event evidence” is enabled, only image-inference frames that reach the immediate-enforcement threshold are encoded by the Agent and submitted to the Service for encrypted storage; the Service revalidates both policy and threshold. The Agent loads image and OCR models from a verified local manifest and does not emit rules, recognized text, scores, or screen content at runtime. Continuous-frame risk fusion and source-window observation are not yet connected, so the current classifier does not automatically close the source application.

The management UI is located at [`apps/karma-ui/`](apps/karma-ui/). Windows builds connect to `KarmaService` through a native named pipe that supports concurrent clients. Administrator passwords, sessions, live Agent/display status, policy revisions, and evidence viewing are all controlled by the Service. If the connection fails, the GUI shows an explicit Service connection error instead of presenting an unknown authentication state as a password-unlock screen. Non-Windows development builds continue to use an isolated local backend.

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

- Exiting the UI, pausing monitoring, changing policies, uninstalling, and granting temporary access all require the administrator password.
- Closing the UI only hides the management interface; it does not stop the Agent or Service.
- Consecutive failures trigger exponential backoff and an audit event.
- A one-time recovery code is provided; it is displayed only once and stored only as a hash.

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

### Windows

- Use the WFP ALE connect/accept layers to create persistent filters based on application path, user, and protocol.
- Atomically switch filter sets when a scheduled period changes.
- If the service crashes, persistent rules maintain the last safe state.
- The basic version can begin with the Windows Firewall API and add a WFP callout for more complex scenarios.

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
- Uninstallation requires the administrator password and UAC approval; in MDM mode, the device policy determines whether removal is permitted.

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
