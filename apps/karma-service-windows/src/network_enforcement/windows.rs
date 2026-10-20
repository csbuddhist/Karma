//! Persistent WFP filters, owned exclusively by Karma's provider and sublayer.

use std::{
    ffi::c_void,
    path::Path,
    ptr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use karma_service_core::ServiceCore;
use windows::{
    Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, FWP_E_ALREADY_EXISTS, HANDLE},
        NetworkManagement::WindowsFilteringPlatform::*,
        Storage::FileSystem::QueryDosDeviceW,
        System::{Rpc::RPC_C_AUTHN_WINNT, SystemInformation::GetSystemDirectoryW},
    },
    core::{GUID, PCWSTR, PWSTR, w},
};

use super::{ApplicationMatcher, FilterBackend, FilterPlan, Reconciler};

const PROVIDER_KEY: GUID = GUID::from_u128(0x9ddf8cd3_8b51_4ac7_9dca_c3cc0f175663);
const SUBLAYER_KEY: GUID = GUID::from_u128(0x0bd6b682_e7ad_4ba7_a97d_8d7ee779ebaa);
const LAYERS: [GUID; 4] = [
    FWPM_LAYER_ALE_AUTH_CONNECT_V4,
    FWPM_LAYER_ALE_AUTH_CONNECT_V6,
    FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V4,
    FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V6,
];
const POLL_INTERVAL: Duration = Duration::from_secs(60);
const TICK: Duration = Duration::from_secs(1);

#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    #[error("WFP {operation} failed (0x{code:08x})")]
    Windows { operation: &'static str, code: u32 },
    #[error("network schedule state is unavailable")]
    Core,
    #[error("network enforcement worker panicked")]
    WorkerPanicked,
}

fn check(operation: &'static str, code: u32) -> Result<(), NetworkError> {
    if code == 0 {
        Ok(())
    } else {
        Err(NetworkError::Windows { operation, code })
    }
}

pub fn start(
    core: Arc<ServiceCore>,
    shutdown: Arc<AtomicBool>,
) -> std::io::Result<JoinHandle<Result<(), NetworkError>>> {
    thread::Builder::new()
        .name("karma-network-enforcement".into())
        .spawn(move || run(&core, &shutdown))
}

pub fn clear_filters() -> Result<(), NetworkError> {
    WfpEngine::open()?.remove()
}

fn run(core: &ServiceCore, shutdown: &AtomicBool) -> Result<(), NetworkError> {
    let mut reconciler = None;
    let mut next_poll = Instant::now();
    while !shutdown.load(Ordering::Acquire) {
        if Instant::now() >= next_poll {
            let result = (|| {
                let enforcement = core
                    .network_enforcement(crate::windows_service::unix_time_ms())
                    .map_err(|_| NetworkError::Core)?;
                if reconciler.is_none() {
                    reconciler = Some(Reconciler::new(WfpEngine::open()?));
                }
                let reconciler = reconciler.as_mut().expect("engine was opened");
                if reconciler.applied.is_some() {
                    reconciler.backend.check_connection()?;
                }
                reconciler.reconcile(FilterPlan::from_restriction(&enforcement.restriction))
            })();
            if let Err(error) = &result {
                eprintln!("KarmaService network enforcement: {error}");
                // Reopen on the next poll (including BFE/RPC restart). Persistent filters
                // survive the old handle; never clear them merely because an update failed.
                reconciler = None;
            }
            if let Err(error) = core
                .record_network_enforcement(result.is_ok(), crate::windows_service::unix_time_ms())
            {
                eprintln!("KarmaService network enforcement audit: {error}");
            }
            next_poll = Instant::now() + POLL_INTERVAL;
        }
        thread::sleep(TICK);
    }
    // An authorized stop/uninstall must not leave persistent network restrictions.
    // A crash never reaches this branch, retaining the last committed policy instead.
    let result = clear_filters();
    let _ = core.record_network_enforcement(result.is_ok(), crate::windows_service::unix_time_ms());
    result
}

struct WfpEngine(HANDLE);

impl WfpEngine {
    fn check_connection(&self) -> Result<(), NetworkError> {
        let mut provider = ptr::null_mut();
        check("check provider", unsafe {
            FwpmProviderGetByKey0(self.0, &PROVIDER_KEY, &mut provider)
        })?;
        let _memory = WfpMemory(provider);
        Ok(())
    }

    fn open() -> Result<Self, NetworkError> {
        let mut handle = HANDLE::default();
        // A non-dynamic session is required for persistent filters. Bound the RPC
        // transaction wait so shutdown cannot hang indefinitely on a competing writer.
        let session = FWPM_SESSION0 {
            txnWaitTimeoutInMSec: 5000,
            ..Default::default()
        };
        check("open", unsafe {
            FwpmEngineOpen0(
                PCWSTR::null(),
                RPC_C_AUTHN_WINNT,
                None,
                Some(&session),
                &mut handle,
            )
        })?;
        Ok(Self(handle))
    }

    fn transaction(
        &self,
        update: impl FnOnce() -> Result<(), NetworkError>,
    ) -> Result<(), NetworkError> {
        check("begin transaction", unsafe {
            FwpmTransactionBegin0(self.0, 0)
        })?;
        let transaction = Transaction(self.0);
        update()?;
        check("commit transaction", unsafe {
            FwpmTransactionCommit0(self.0)
        })?;
        std::mem::forget(transaction);
        Ok(())
    }

    fn ensure_objects(&self) -> Result<(), NetworkError> {
        let mut provider_key = PROVIDER_KEY;
        let provider = FWPM_PROVIDER0 {
            providerKey: provider_key,
            displayData: display_data(),
            flags: FWPM_PROVIDER_FLAG_PERSISTENT,
            ..Default::default()
        };
        let code = unsafe { FwpmProviderAdd0(self.0, &provider, None) };
        if code != FWP_E_ALREADY_EXISTS.0 as u32 {
            check("add provider", code)?;
        }
        let sublayer = FWPM_SUBLAYER0 {
            subLayerKey: SUBLAYER_KEY,
            displayData: display_data(),
            flags: FWPM_SUBLAYER_FLAG_PERSISTENT,
            providerKey: &mut provider_key,
            weight: 0x8000,
            ..Default::default()
        };
        let code = unsafe { FwpmSubLayerAdd0(self.0, &sublayer, None) };
        if code != FWP_E_ALREADY_EXISTS.0 as u32 {
            check("add sublayer", code)?;
        }
        Ok(())
    }

    fn clear_filters(&self) -> Result<(), NetworkError> {
        let mut provider_key = PROVIDER_KEY;
        for layer in LAYERS {
            let template = FWPM_FILTER_ENUM_TEMPLATE0 {
                providerKey: &mut provider_key,
                layerKey: layer,
                actionMask: u32::MAX,
                ..Default::default()
            };
            let mut handle = HANDLE::default();
            check("create enumeration", unsafe {
                FwpmFilterCreateEnumHandle0(self.0, Some(&template), &mut handle)
            })?;
            let enumeration = Enumeration {
                engine: self.0,
                handle,
            };
            let mut ids = Vec::new();
            loop {
                let mut entries = ptr::null_mut();
                let mut count = 0;
                check("enumerate filters", unsafe {
                    FwpmFilterEnum0(self.0, enumeration.handle, 128, &mut entries, &mut count)
                })?;
                let memory = WfpMemory(entries);
                for index in 0..count as usize {
                    // WFP owns this pointer array until FwpmFreeMemory0.
                    let filter = unsafe { &**memory.0.add(index) };
                    if filter.subLayerKey == SUBLAYER_KEY {
                        ids.push(filter.filterId);
                    }
                }
                if count < 128 {
                    break;
                }
            }
            for id in ids {
                check("delete filter", unsafe {
                    FwpmFilterDeleteById0(self.0, id)
                })?;
            }
        }
        Ok(())
    }

    fn add(
        &self,
        layer: GUID,
        action: FWP_ACTION_TYPE,
        app: Option<(&mut FWP_BYTE_BLOB, FWP_MATCH_TYPE)>,
    ) -> Result<(), NetworkError> {
        let mut provider_key = PROVIDER_KEY;
        let mut condition = FWPM_FILTER_CONDITION0::default();
        let has_condition = app.is_some();
        if let Some((blob, match_type)) = app {
            condition = FWPM_FILTER_CONDITION0 {
                fieldKey: FWPM_CONDITION_ALE_APP_ID,
                matchType: match_type,
                conditionValue: FWP_CONDITION_VALUE0 {
                    r#type: FWP_BYTE_BLOB_TYPE,
                    Anonymous: FWP_CONDITION_VALUE0_0 { byteBlob: blob },
                },
            };
        }
        let filter = FWPM_FILTER0 {
            displayData: display_data(),
            flags: FWPM_FILTER_FLAG_PERSISTENT,
            providerKey: &mut provider_key,
            layerKey: layer,
            subLayerKey: SUBLAYER_KEY,
            weight: FWP_VALUE0 {
                r#type: FWP_UINT8,
                Anonymous: FWP_VALUE0_0 {
                    uint8: if action == FWP_ACTION_PERMIT { 15 } else { 1 },
                },
            },
            numFilterConditions: u32::from(has_condition),
            filterCondition: if has_condition {
                &mut condition
            } else {
                ptr::null_mut()
            },
            action: FWPM_ACTION0 {
                r#type: action,
                ..Default::default()
            },
            ..Default::default()
        };
        // Exemptions are higher-weight soft permits in the SAME sublayer. Do not
        // set CLEAR_ACTION_RIGHT: other firewall/security providers may still block.
        check("add filter", unsafe {
            FwpmFilterAdd0(self.0, &filter, None, None)
        })
    }

    fn remove(&mut self) -> Result<(), NetworkError> {
        self.transaction(|| {
            self.ensure_objects()?;
            self.clear_filters()?;
            check("delete sublayer", unsafe {
                FwpmSubLayerDeleteByKey0(self.0, &SUBLAYER_KEY)
            })?;
            check("delete provider", unsafe {
                FwpmProviderDeleteByKey0(self.0, &PROVIDER_KEY)
            })
        })
    }
}

impl FilterBackend for WfpEngine {
    type Error = NetworkError;

    fn replace(&mut self, plan: &FilterPlan) -> Result<(), NetworkError> {
        // Prepare all app identities before touching the installed set. Suffix
        // conditions cover processes not yet started; no process enumeration race.
        let mut blocked = plan
            .applications
            .iter()
            .map(application_id)
            .collect::<Result<Vec<_>, _>>()?;
        let mut exemptions = if plan.is_empty() {
            Vec::new()
        } else {
            exemption_ids()?
        };
        self.transaction(|| {
            self.ensure_objects()?;
            self.clear_filters()?;
            if plan.is_empty() {
                return Ok(());
            }
            for layer in LAYERS {
                for app in &mut exemptions {
                    self.add(
                        layer,
                        FWP_ACTION_PERMIT,
                        Some((&mut app.blob(), FWP_MATCH_EQUAL)),
                    )?;
                }
                if plan.block_all {
                    self.add(layer, FWP_ACTION_BLOCK, None)?;
                } else {
                    for (data, match_type) in &mut blocked {
                        self.add(
                            layer,
                            FWP_ACTION_BLOCK,
                            Some((&mut data.blob(), *match_type)),
                        )?;
                    }
                }
            }
            Ok(())
        })
    }
}

fn display_data() -> FWPM_DISPLAY_DATA0 {
    FWPM_DISPLAY_DATA0 {
        name: PWSTR(
            w!("Karma network schedules / Karma 网络时段")
                .as_ptr()
                .cast_mut(),
        ),
        description: PWSTR(
            w!("Karma managed network restriction / Karma 管理的网络限制")
                .as_ptr()
                .cast_mut(),
        ),
    }
}

struct AppId(Vec<u16>);

impl AppId {
    fn text(value: &str) -> Self {
        Self(value.encode_utf16().chain(Some(0)).collect())
    }

    fn blob(&mut self) -> FWP_BYTE_BLOB {
        FWP_BYTE_BLOB {
            size: (self.0.len() * 2) as u32,
            data: self.0.as_mut_ptr().cast(),
        }
    }

    fn full_path(path: &str) -> Result<Self, NetworkError> {
        let path_wide = Self::text(path);
        let mut blob = ptr::null_mut();
        let code = unsafe { FwpmGetAppIdFromFileName0(PCWSTR(path_wide.0.as_ptr()), &mut blob) };
        if code == 0 {
            let memory = WfpMemory(blob);
            let blob = unsafe { &*memory.0 };
            let data = unsafe {
                std::slice::from_raw_parts(blob.data.cast::<u16>(), blob.size as usize / 2)
            };
            return Ok(Self(data.to_vec()));
        }
        // An absent program still needs a filter before its first launch. Resolve
        // the drive's NT device name without requiring the executable to exist.
        if (code == ERROR_FILE_NOT_FOUND.0 || code == ERROR_PATH_NOT_FOUND.0)
            && path.as_bytes().get(1) == Some(&b':')
        {
            let drive = Self::text(&path[..2]);
            let mut device = vec![0_u16; 32768];
            let size = unsafe { QueryDosDeviceW(PCWSTR(drive.0.as_ptr()), Some(&mut device)) };
            if size != 0 {
                let end = device
                    .iter()
                    .position(|value| *value == 0)
                    .unwrap_or(size as usize);
                let device = String::from_utf16_lossy(&device[..end]).to_lowercase();
                return Ok(Self::text(&format!("{device}{}", &path[2..])));
            }
        }
        check("resolve application identity", code)?;
        unreachable!()
    }
}

fn application_id(matcher: &ApplicationMatcher) -> Result<(AppId, FWP_MATCH_TYPE), NetworkError> {
    match matcher {
        ApplicationMatcher::FullPath(path) => Ok((AppId::full_path(path)?, FWP_MATCH_EQUAL)),
        // Despite its name, FWP_MATCH_PREFIX is documented as a suffix match.
        ApplicationMatcher::Suffix(path) => Ok((AppId::text(path), FWP_MATCH_PREFIX)),
    }
}

fn exemption_ids() -> Result<Vec<AppId>, NetworkError> {
    let mut system = vec![0_u16; 32768];
    let length = unsafe { GetSystemDirectoryW(Some(&mut system)) } as usize;
    if length == 0 || length >= system.len() {
        return Err(NetworkError::Windows {
            operation: "system directory",
            code: 1,
        });
    }
    let system = String::from_utf16_lossy(&system[..length]);
    // Exact OS infrastructure paths only. Do not exempt the whole Windows tree
    // (which includes browsers, curl and PowerShell) or arbitrary names elsewhere.
    let paths = [
        "svchost.exe",
        "lsass.exe",
        "services.exe",
        "wininit.exe",
        "ntoskrnl.exe",
    ]
    .map(|name| format!("{system}\\{name}").to_lowercase());
    let mut ids = paths
        .iter()
        .map(|path| AppId::full_path(path))
        .collect::<Result<Vec<_>, _>>()?;
    // Kernel networking may use the special System identity rather than an image path.
    ids.push(AppId::text("system"));
    let executable = std::env::current_exe().map_err(|_| NetworkError::Windows {
        operation: "installation path",
        code: 1,
    })?;
    ids.push(AppId::full_path(
        &executable.to_string_lossy().to_lowercase(),
    )?);
    let directory = executable.parent().unwrap_or(Path::new(""));
    for name in [
        "KarmaService.exe",
        "KarmaGuardian.exe",
        "KarmaControl.exe",
        "karma-agent-windows.exe",
        "karma-ui.exe",
    ] {
        ids.push(AppId::full_path(
            &directory.join(name).to_string_lossy().to_lowercase(),
        )?);
    }
    Ok(ids)
}

impl Drop for WfpEngine {
    fn drop(&mut self) {
        unsafe {
            let _ = FwpmEngineClose0(self.0);
        }
    }
}

struct Transaction(HANDLE);
impl Drop for Transaction {
    fn drop(&mut self) {
        unsafe {
            let _ = FwpmTransactionAbort0(self.0);
        }
    }
}

struct Enumeration {
    engine: HANDLE,
    handle: HANDLE,
}
impl Drop for Enumeration {
    fn drop(&mut self) {
        unsafe {
            let _ = FwpmFilterDestroyEnumHandle0(self.engine, self.handle);
        }
    }
}

struct WfpMemory<T>(*mut T);
impl<T> Drop for WfpMemory<T> {
    fn drop(&mut self) {
        let mut memory = self.0.cast::<c_void>();
        unsafe {
            FwpmFreeMemory0(&mut memory);
        }
    }
}
