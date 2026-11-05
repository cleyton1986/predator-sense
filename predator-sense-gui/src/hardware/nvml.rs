//! The NVIDIA GPU temperature through NVML, for the work that runs with the
//! window closed (the fan reconciler and the temperature alerts).
//!
//! It used to come from `nvidia-smi`, started every couple of seconds. Each
//! run initialised NVML from scratch, about 80 driver calls, and on a PH16-71
//! those calls go through the firmware and raise SMIs, which stall every core
//! (measured: about 30 SMIs a second with the app idle, none with it quit).
//! Here the library is loaded once with `dlopen`, initialised once, and each
//! reading is one `nvmlDeviceGetTemperature` call. The GPU pages' full
//! metrics ([`Details`]) come from the same session, a few dozen calls with no
//! process started, instead of a `nvidia-smi` run every two seconds.
//!
//! Nothing links against NVML at build time. Without `libnvidia-ml.so.1` there
//! is simply no GPU temperature, as there was without `nvidia-smi`.
//!
//! The RTD3 rule of issue #79 holds: the caller passes whether the dGPU may be
//! touched (`nvidia::live_query_is_safe`), NVML is never initialised or asked
//! anything while it may not, and a session open when the GPU starts to
//! suspend is shut down so this process holds nothing on it.

use std::ffi::{c_char, c_int, c_uint, c_void, CString};
use std::time::{Duration, Instant};

/// Why NVML could not be opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenError {
    /// The library or one of its functions is missing: it will not appear
    /// while the app runs, so there is no point trying again.
    Unavailable,
    /// The library is there but initialising it or finding the GPU failed,
    /// which a driver reload can change.
    Failed,
}

/// What NVML has to do, so the session rules can be tested with a fake.
pub trait Backend {
    fn open(&mut self) -> Result<(), OpenError>;
    /// The GPU temperature in degrees C, `None` when the call failed.
    fn temperature(&mut self) -> Option<u32>;
    /// The power limit range in milliwatts, `None` when the GPU does not say.
    fn power_limits(&mut self) -> Option<(u32, u32)>;
    /// Everything the GPU pages show; `None` when the temperature, the one
    /// field every GPU gives, cannot be read.
    fn details(&mut self) -> Option<Details>;
    fn close(&mut self);
}

/// The full metrics, as NVML gives them. A field the GPU or the driver does
/// not give is `None`, where `nvidia-smi` printed `[N/A]`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Details {
    pub temperature: u32,
    pub name: Option<String>,
    pub driver: Option<String>,
    pub vbios: Option<String>,
    /// Total, used and free, in bytes. Used leaves out the driver's reserved
    /// memory where the driver can say so, as `nvidia-smi` does.
    pub memory: Option<(u64, u64, u64)>,
    /// Current graphics and memory clocks in MHz.
    pub clocks: (Option<u32>, Option<u32>),
    /// Maximum graphics and memory clocks in MHz.
    pub max_clocks: (Option<u32>, Option<u32>),
    /// GPU and memory utilisation in percent.
    pub utilization: Option<(u32, u32)>,
    pub power_draw_mw: Option<u32>,
    pub power_limit_mw: Option<u32>,
    /// Minimum and maximum power limit in milliwatts.
    pub power_limits_mw: Option<(u32, u32)>,
    pub fan_pct: Option<u32>,
    /// The performance state number, 0 for P0.
    pub pstate: Option<u32>,
    pub pcie_gen: Option<u32>,
    pub pcie_width: Option<u32>,
    /// Whether the inforom has a power management object (`nvidia-smi`'s
    /// `inforom.pwr` not being `[N/A]`).
    pub power_object: bool,
}

/// Consecutive failures (opens or readings) after which NVML is left alone
/// for [`RETRY_AFTER`]. Every open is a full initialisation, the expensive
/// part this module exists to avoid repeating.
pub const MAX_FAILURES: u32 = 3;

/// How long NVML is left alone after [`MAX_FAILURES`], between single retries.
/// A transient fault (a GPU lost around a resume, a driver reload) then costs
/// the fan curve its GPU input for minutes, not for the rest of the run.
pub const RETRY_AFTER: Duration = Duration::from_secs(300);

/// One NVML session and the rules for opening, reading and releasing it.
pub struct Session<B: Backend> {
    backend: B,
    open: bool,
    failures: u32,
    /// The library or a function is missing: never tried again.
    unavailable: bool,
    /// Backing off after [`MAX_FAILURES`]: no attempt before this.
    retry_at: Option<Instant>,
    /// A line for the log, taken by [`Session::take_log`].
    log: Option<&'static str>,
}

impl<B: Backend> Session<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            open: false,
            failures: 0,
            unavailable: false,
            retry_at: None,
            log: None,
        }
    }

    /// The temperature now, or `None`. `safe` is whether the dGPU may be
    /// touched at all: when it may not, nothing is asked and an open session
    /// is released.
    pub fn read(&mut self, safe: bool, now: Instant) -> Option<f64> {
        if !self.ensure_open(safe, now) {
            return None;
        }
        let celsius = self.backend.temperature();
        self.settle(celsius.is_some(), now);
        celsius.map(f64::from)
    }

    /// The full metrics now, or `None`, under the same rules as [`Session::read`].
    pub fn details(&mut self, safe: bool, now: Instant) -> Option<Details> {
        if !self.ensure_open(safe, now) {
            return None;
        }
        let details = self.backend.details();
        self.settle(details.is_some(), now);
        details
    }

    /// Opens the session if it may be and is not open yet; whether it is open.
    fn ensure_open(&mut self, safe: bool, now: Instant) -> bool {
        if !safe {
            self.release();
            return false;
        }
        if self.unavailable || self.retry_at.is_some_and(|at| now < at) {
            return false;
        }
        if !self.open {
            match self.backend.open() {
                Ok(()) => self.open = true,
                Err(OpenError::Unavailable) => {
                    self.unavailable = true;
                    self.log = Some(
                        "NVML: libnvidia-ml.so.1 or a function in it is missing; \
                         no GPU temperature this run",
                    );
                    return false;
                }
                Err(OpenError::Failed) => {
                    self.failed(now);
                    return false;
                }
            }
        }
        true
    }

    /// Counts a reading that worked or failed.
    fn settle(&mut self, worked: bool, now: Instant) {
        if worked {
            if self.failures >= MAX_FAILURES {
                self.log = Some("NVML: GPU temperature readable again");
            }
            self.failures = 0;
            self.retry_at = None;
        } else {
            // A GPU that fell off the bus or a driver reload: start over
            // on the next reading rather than asking a dead handle.
            self.release();
            self.failed(now);
        }
    }

    fn failed(&mut self, now: Instant) {
        self.failures = self.failures.saturating_add(1);
        if self.failures >= MAX_FAILURES {
            if self.failures == MAX_FAILURES {
                self.log = Some(
                    "NVML: the GPU temperature failed 3 times in a row; \
                     trying again every 5 minutes",
                );
            }
            self.retry_at = Some(now + RETRY_AFTER);
        }
    }

    /// The power limit range in watts, asked of an open session only.
    pub fn power_limits(&mut self) -> Option<(f64, f64)> {
        if !self.open {
            return None;
        }
        let (min, max) = self.backend.power_limits()?;
        (max > 0).then(|| (f64::from(min) / 1000.0, f64::from(max) / 1000.0))
    }

    /// Shut NVML down if it is open. The next reading opens it again.
    pub fn release(&mut self) {
        if self.open {
            self.backend.close();
            self.open = false;
        }
    }

    /// The line to log about a change in what NVML can do, once.
    pub fn take_log(&mut self) -> Option<&'static str> {
        self.log.take()
    }

    #[cfg(test)]
    fn is_open(&self) -> bool {
        self.open
    }
}

type NvmlReturn = c_int;
type Device = *mut c_void;

const NVML_SUCCESS: NvmlReturn = 0;
const NVML_TEMPERATURE_GPU: c_uint = 0;

struct Functions {
    init: unsafe extern "C" fn() -> NvmlReturn,
    shutdown: unsafe extern "C" fn() -> NvmlReturn,
    handle_by_bus_id: unsafe extern "C" fn(*const c_char, *mut Device) -> NvmlReturn,
    handle_by_index: unsafe extern "C" fn(c_uint, *mut Device) -> NvmlReturn,
    temperature: unsafe extern "C" fn(Device, c_uint, *mut c_uint) -> NvmlReturn,
    /// Optional: the temperature does not depend on it.
    power_limits: Option<unsafe extern "C" fn(Device, *mut c_uint, *mut c_uint) -> NvmlReturn>,
    /// Optional, for [`Details`]: a driver without one of them leaves that
    /// field empty.
    extra: Extra,
}

type TextFn = unsafe extern "C" fn(Device, *mut c_char, c_uint) -> NvmlReturn;
type UintFn = unsafe extern "C" fn(Device, *mut c_uint) -> NvmlReturn;
type ClockFn = unsafe extern "C" fn(Device, c_uint, *mut c_uint) -> NvmlReturn;
type StructFn = unsafe extern "C" fn(Device, *mut c_void) -> NvmlReturn;

#[derive(Default)]
struct Extra {
    name: Option<TextFn>,
    driver: Option<unsafe extern "C" fn(*mut c_char, c_uint) -> NvmlReturn>,
    vbios: Option<TextFn>,
    memory_v2: Option<StructFn>,
    memory: Option<StructFn>,
    clock: Option<ClockFn>,
    max_clock: Option<ClockFn>,
    utilization: Option<StructFn>,
    power_usage: Option<UintFn>,
    power_limit: Option<UintFn>,
    fan: Option<UintFn>,
    pstate: Option<UintFn>,
    pcie_gen: Option<UintFn>,
    pcie_width: Option<UintFn>,
    inforom: Option<unsafe extern "C" fn(Device, c_uint, *mut c_char, c_uint) -> NvmlReturn>,
}

const NVML_CLOCK_GRAPHICS: c_uint = 0;
const NVML_CLOCK_MEM: c_uint = 2;
const NVML_INFOROM_POWER: c_uint = 2;
/// `nvmlPstates_t`'s value for an unknown state.
const NVML_PSTATE_UNKNOWN: c_uint = 32;

/// `nvmlMemory_t`.
#[repr(C)]
#[derive(Default)]
struct Memory {
    total: u64,
    free: u64,
    used: u64,
}

/// `nvmlMemory_v2_t`.
#[repr(C)]
#[derive(Default)]
struct MemoryV2 {
    version: c_uint,
    total: u64,
    reserved: u64,
    free: u64,
    used: u64,
}

/// `nvmlUtilization_t`.
#[repr(C)]
#[derive(Default)]
struct Utilization {
    gpu: c_uint,
    memory: c_uint,
}

/// A symbol as the function pointer type `F`.
///
/// # Safety
/// `F` must be a function pointer type matching the signature NVML documents
/// for the symbol at `pointer`.
unsafe fn cast<F: Copy>(pointer: *mut c_void) -> F {
    debug_assert_eq!(std::mem::size_of::<F>(), std::mem::size_of::<*mut c_void>());
    // SAFETY: as the caller promises; both are one pointer wide.
    unsafe { std::mem::transmute_copy(&pointer) }
}

/// Text from an NVML call that fills a buffer, `None` when it fails or is empty.
fn text(call: impl FnOnce(*mut c_char, c_uint) -> NvmlReturn) -> Option<String> {
    let mut buffer = [0 as c_char; 96];
    if call(buffer.as_mut_ptr(), buffer.len() as c_uint) != NVML_SUCCESS {
        return None;
    }
    // SAFETY: NVML wrote a NUL-terminated string into the buffer (and the
    // last byte is never overwritten past its length).
    let value = unsafe { std::ffi::CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .trim()
        .to_string();
    (!value.is_empty()).then_some(value)
}

/// The real library. Loaded on the first open and never unloaded: NVML's own
/// threads do not survive `dlclose`.
pub struct Nvml {
    /// The PCI address of the GPU to read, as sysfs names it.
    bus_id: Option<String>,
    functions: Option<Functions>,
    device: Device,
    /// The fields that cannot change while a session is open (name, driver,
    /// VBIOS, maximum clocks, power limit range, inforom power object), read on the first
    /// [`Backend::details`] of each session: the inforom and VBIOS calls are
    /// the slow part of a full reading.
    fixed: Option<Fixed>,
}

#[derive(Clone)]
struct Fixed {
    name: Option<String>,
    driver: Option<String>,
    vbios: Option<String>,
    max_clocks: (Option<u32>, Option<u32>),
    power_limits_mw: Option<(u32, u32)>,
    power_object: bool,
}

impl Nvml {
    pub fn new(bus_id: Option<String>) -> Self {
        Self {
            bus_id,
            functions: None,
            device: std::ptr::null_mut(),
            fixed: None,
        }
    }

    fn load() -> Option<Functions> {
        // SAFETY: plain dlopen/dlsym on a constant name; every symbol is
        // checked for null before it is turned into a function pointer of
        // the signature NVML documents for it.
        unsafe {
            let library = libc::dlopen(
                c"libnvidia-ml.so.1".as_ptr(),
                libc::RTLD_NOW | libc::RTLD_LOCAL,
            );
            if library.is_null() {
                return None;
            }
            let symbol = |name: &std::ffi::CStr| {
                let pointer = libc::dlsym(library, name.as_ptr());
                (!pointer.is_null()).then_some(pointer)
            };
            let functions = (|| {
                Some(Functions {
                    init: std::mem::transmute::<
                        *mut c_void,
                        unsafe extern "C" fn() -> NvmlReturn,
                    >(symbol(c"nvmlInit_v2")?),
                    shutdown: std::mem::transmute::<
                        *mut c_void,
                        unsafe extern "C" fn() -> NvmlReturn,
                    >(symbol(c"nvmlShutdown")?),
                    handle_by_bus_id: std::mem::transmute::<
                        *mut c_void,
                        unsafe extern "C" fn(*const c_char, *mut Device) -> NvmlReturn,
                    >(symbol(c"nvmlDeviceGetHandleByPciBusId_v2")?),
                    handle_by_index: std::mem::transmute::<
                        *mut c_void,
                        unsafe extern "C" fn(c_uint, *mut Device) -> NvmlReturn,
                    >(symbol(c"nvmlDeviceGetHandleByIndex_v2")?),
                    temperature: std::mem::transmute::<
                        *mut c_void,
                        unsafe extern "C" fn(Device, c_uint, *mut c_uint) -> NvmlReturn,
                    >(symbol(c"nvmlDeviceGetTemperature")?),
                    power_limits: symbol(c"nvmlDeviceGetPowerManagementLimitConstraints").map(
                        |pointer| {
                            std::mem::transmute::<
                                *mut c_void,
                                unsafe extern "C" fn(Device, *mut c_uint, *mut c_uint) -> NvmlReturn,
                            >(pointer)
                        },
                    ),
                    extra: Extra {
                        name: symbol(c"nvmlDeviceGetName").map(|p| cast(p)),
                        driver: symbol(c"nvmlSystemGetDriverVersion").map(|p| cast(p)),
                        vbios: symbol(c"nvmlDeviceGetVbiosVersion").map(|p| cast(p)),
                        memory_v2: symbol(c"nvmlDeviceGetMemoryInfo_v2").map(|p| cast(p)),
                        memory: symbol(c"nvmlDeviceGetMemoryInfo").map(|p| cast(p)),
                        clock: symbol(c"nvmlDeviceGetClockInfo").map(|p| cast(p)),
                        max_clock: symbol(c"nvmlDeviceGetMaxClockInfo").map(|p| cast(p)),
                        utilization: symbol(c"nvmlDeviceGetUtilizationRates").map(|p| cast(p)),
                        power_usage: symbol(c"nvmlDeviceGetPowerUsage").map(|p| cast(p)),
                        power_limit: symbol(c"nvmlDeviceGetPowerManagementLimit").map(|p| cast(p)),
                        fan: symbol(c"nvmlDeviceGetFanSpeed").map(|p| cast(p)),
                        pstate: symbol(c"nvmlDeviceGetPerformanceState").map(|p| cast(p)),
                        pcie_gen: symbol(c"nvmlDeviceGetCurrPcieLinkGeneration").map(|p| cast(p)),
                        pcie_width: symbol(c"nvmlDeviceGetCurrPcieLinkWidth").map(|p| cast(p)),
                        inforom: symbol(c"nvmlDeviceGetInforomVersion").map(|p| cast(p)),
                    },
                })
            })();
            if functions.is_none() {
                // A library missing a function is never used, and nothing of
                // it has run yet, so it can be unloaded safely.
                libc::dlclose(library);
            }
            functions
        }
    }
}

impl Backend for Nvml {
    fn open(&mut self) -> Result<(), OpenError> {
        if self.functions.is_none() {
            self.functions = Some(Self::load().ok_or(OpenError::Unavailable)?);
        }
        let functions = self.functions.as_ref().ok_or(OpenError::Unavailable)?;
        // SAFETY: functions loaded above with their documented signatures;
        // `device` is a valid out pointer.
        unsafe {
            if (functions.init)() != NVML_SUCCESS {
                return Err(OpenError::Failed);
            }
            let mut device: Device = std::ptr::null_mut();
            let by_bus = self
                .bus_id
                .as_deref()
                .and_then(|bus| CString::new(bus).ok())
                .is_some_and(|bus| {
                    (functions.handle_by_bus_id)(bus.as_ptr(), &mut device) == NVML_SUCCESS
                });
            if !by_bus && (functions.handle_by_index)(0, &mut device) != NVML_SUCCESS {
                (functions.shutdown)();
                return Err(OpenError::Failed);
            }
            self.device = device;
        }
        Ok(())
    }

    fn temperature(&mut self) -> Option<u32> {
        let functions = self.functions.as_ref()?;
        let mut celsius: c_uint = 0;
        // SAFETY: `device` came from a successful handle lookup in this
        // still-initialised session.
        let status =
            unsafe { (functions.temperature)(self.device, NVML_TEMPERATURE_GPU, &mut celsius) };
        (status == NVML_SUCCESS).then_some(celsius)
    }

    fn power_limits(&mut self) -> Option<(u32, u32)> {
        let query = self.functions.as_ref()?.power_limits?;
        let (mut min, mut max): (c_uint, c_uint) = (0, 0);
        // SAFETY: as for `temperature`; both out pointers are valid.
        let status = unsafe { query(self.device, &mut min, &mut max) };
        (status == NVML_SUCCESS).then_some((min, max))
    }

    fn details(&mut self) -> Option<Details> {
        let temperature = self.temperature()?;
        let power_limits_mw = match &self.fixed {
            Some(fixed) => fixed.power_limits_mw,
            None => self.power_limits().filter(|&(_, max)| max > 0),
        };
        let functions = self.functions.as_ref()?;
        let extra = &functions.extra;
        let device = self.device;
        // SAFETY for every call below: each function was loaded with the
        // signature NVML documents for it, `device` came from a successful
        // handle lookup in this still-initialised session, and every out
        // pointer is valid for the type NVML writes.
        let uint = |f: Option<UintFn>| {
            let f = f?;
            let mut value: c_uint = 0;
            (unsafe { f(device, &mut value) } == NVML_SUCCESS).then_some(value)
        };
        let clock = |f: Option<ClockFn>, kind: c_uint| {
            let f = f?;
            let mut value: c_uint = 0;
            (unsafe { f(device, kind, &mut value) } == NVML_SUCCESS).then_some(value)
        };
        let device_text = |f: Option<TextFn>| {
            let f = f?;
            text(|buffer, length| unsafe { f(device, buffer, length) })
        };
        let memory = extra
            .memory_v2
            .and_then(|f| {
                let mut memory = MemoryV2 {
                    // NVML_STRUCT_VERSION(Memory, 2).
                    version: std::mem::size_of::<MemoryV2>() as c_uint | (2 << 24),
                    ..MemoryV2::default()
                };
                (unsafe { f(device, (&mut memory as *mut MemoryV2).cast()) } == NVML_SUCCESS)
                    .then_some((memory.total, memory.used, memory.free))
            })
            .or_else(|| {
                let f = extra.memory?;
                let mut memory = Memory::default();
                (unsafe { f(device, (&mut memory as *mut Memory).cast()) } == NVML_SUCCESS)
                    .then_some((memory.total, memory.used, memory.free))
            });
        let utilization = extra.utilization.and_then(|f| {
            let mut rates = Utilization::default();
            (unsafe { f(device, (&mut rates as *mut Utilization).cast()) } == NVML_SUCCESS)
                .then_some((rates.gpu, rates.memory))
        });
        let fixed = match &self.fixed {
            Some(fixed) => fixed.clone(),
            None => {
                let fixed = Fixed {
                    name: device_text(extra.name),
                    driver: extra
                        .driver
                        .and_then(|f| text(|buffer, length| unsafe { f(buffer, length) })),
                    vbios: device_text(extra.vbios),
                    max_clocks: (
                        clock(extra.max_clock, NVML_CLOCK_GRAPHICS),
                        clock(extra.max_clock, NVML_CLOCK_MEM),
                    ),
                    power_limits_mw,
                    power_object: extra.inforom.is_some_and(|f| {
                        text(|buffer, length| unsafe {
                            f(device, NVML_INFOROM_POWER, buffer, length)
                        })
                        .is_some()
                    }),
                };
                self.fixed = Some(fixed.clone());
                fixed
            }
        };
        Some(Details {
            temperature,
            name: fixed.name,
            driver: fixed.driver,
            vbios: fixed.vbios,
            memory,
            clocks: (
                clock(extra.clock, NVML_CLOCK_GRAPHICS),
                clock(extra.clock, NVML_CLOCK_MEM),
            ),
            max_clocks: fixed.max_clocks,
            utilization,
            power_draw_mw: uint(extra.power_usage),
            power_limit_mw: uint(extra.power_limit),
            power_limits_mw,
            fan_pct: uint(extra.fan),
            pstate: uint(extra.pstate).filter(|&state| state != NVML_PSTATE_UNKNOWN),
            pcie_gen: uint(extra.pcie_gen),
            pcie_width: uint(extra.pcie_width),
            power_object: fixed.power_object,
        })
    }

    fn close(&mut self) {
        if let Some(functions) = self.functions.as_ref() {
            // SAFETY: balances the successful init in `open`.
            unsafe {
                (functions.shutdown)();
            }
        }
        self.device = std::ptr::null_mut();
        self.fixed = None;
    }
}

#[cfg(test)]
mod tests {
    use super::{Backend, Details, OpenError, Session, MAX_FAILURES, RETRY_AFTER};
    use std::time::{Duration, Instant};

    #[derive(Default)]
    struct Fake {
        open_results: Vec<Result<(), OpenError>>,
        temperatures: Vec<Option<u32>>,
        opens: u32,
        reads: u32,
        closes: u32,
        limits: Option<(u32, u32)>,
    }

    impl Backend for Fake {
        fn open(&mut self) -> Result<(), OpenError> {
            self.opens += 1;
            if self.open_results.is_empty() {
                Ok(())
            } else {
                self.open_results.remove(0)
            }
        }
        fn temperature(&mut self) -> Option<u32> {
            self.reads += 1;
            if self.temperatures.is_empty() {
                Some(50)
            } else {
                self.temperatures.remove(0)
            }
        }
        fn power_limits(&mut self) -> Option<(u32, u32)> {
            self.limits
        }
        fn details(&mut self) -> Option<Details> {
            let temperature = self.temperature()?;
            Some(Details {
                temperature,
                ..Details::default()
            })
        }
        fn close(&mut self) {
            self.closes += 1;
        }
    }

    /// Against the real library, on a machine whose dGPU is awake:
    /// `cargo test real_nvml -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_nvml_reads_a_temperature() {
        let bus = crate::hardware::nvidia::primary_bus_id();
        let mut session = Session::new(super::Nvml::new(bus.clone()));
        let started = std::time::Instant::now();
        let first = session.read(true, Instant::now());
        let opened = started.elapsed();
        let started = std::time::Instant::now();
        let second = session.read(true, Instant::now());
        let read = started.elapsed();
        eprintln!("bus {bus:?}: {first:?} (open + read {opened:?}), {second:?} (read {read:?})");
        eprintln!("power limits: {:?}", session.power_limits());
        let started = std::time::Instant::now();
        let details = session.details(true, Instant::now());
        eprintln!("details ({:?}): {details:#?}", started.elapsed());
        let started = std::time::Instant::now();
        session.details(true, Instant::now());
        eprintln!("details again: {:?}", started.elapsed());
        assert!(second.is_some_and(|celsius| celsius > 0.0));
        assert!(details.is_some_and(|d| d.name.is_some() && d.memory.is_some()));
    }

    #[test]
    fn opens_once_and_reads_many_times() {
        let mut session = Session::new(Fake::default());
        for _ in 0..5 {
            assert_eq!(session.read(true, Instant::now()), Some(50.0));
        }
        assert_eq!(session.backend.opens, 1);
        assert_eq!(session.backend.reads, 5);
        assert_eq!(session.backend.closes, 0);
    }

    #[test]
    fn never_touches_a_gpu_that_may_not_be_woken() {
        let mut session = Session::new(Fake::default());
        assert_eq!(session.read(false, Instant::now()), None);
        assert_eq!(session.backend.opens, 0);
        assert_eq!(session.backend.reads, 0);
    }

    #[test]
    fn releases_the_session_when_the_gpu_starts_to_suspend() {
        let mut session = Session::new(Fake::default());
        assert_eq!(session.read(true, Instant::now()), Some(50.0));
        assert!(session.is_open());
        assert_eq!(session.read(false, Instant::now()), None);
        assert!(!session.is_open());
        assert_eq!(session.backend.closes, 1);
        // Awake again: a fresh session.
        assert_eq!(session.read(true, Instant::now()), Some(50.0));
        assert_eq!(session.backend.opens, 2);
    }

    #[test]
    fn a_missing_library_is_tried_once() {
        let mut session = Session::new(Fake {
            open_results: vec![Err(OpenError::Unavailable)],
            ..Fake::default()
        });
        for _ in 0..4 {
            assert_eq!(session.read(true, Instant::now()), None);
        }
        assert_eq!(session.backend.opens, 1);
    }

    #[test]
    fn backs_off_after_repeated_failed_opens() {
        let mut session = Session::new(Fake {
            open_results: vec![Err(OpenError::Failed); 10],
            ..Fake::default()
        });
        for _ in 0..10 {
            assert_eq!(session.read(true, Instant::now()), None);
        }
        assert_eq!(session.backend.opens, MAX_FAILURES);
    }

    #[test]
    fn a_failed_open_followed_by_a_good_one_recovers() {
        let mut session = Session::new(Fake {
            open_results: vec![Err(OpenError::Failed), Ok(())],
            ..Fake::default()
        });
        assert_eq!(session.read(true, Instant::now()), None);
        assert_eq!(session.read(true, Instant::now()), Some(50.0));
        assert_eq!(session.read(true, Instant::now()), Some(50.0));
        assert_eq!(session.backend.opens, 2);
    }

    #[test]
    fn backs_off_when_every_reading_fails() {
        let mut session = Session::new(Fake {
            temperatures: vec![None; 10],
            ..Fake::default()
        });
        for _ in 0..10 {
            assert_eq!(session.read(true, Instant::now()), None);
        }
        assert_eq!(session.backend.opens, MAX_FAILURES);
    }

    #[test]
    fn a_failed_reading_closes_and_reopens() {
        let mut session = Session::new(Fake {
            temperatures: vec![Some(41), None, Some(43)],
            ..Fake::default()
        });
        assert_eq!(session.read(true, Instant::now()), Some(41.0));
        assert_eq!(session.read(true, Instant::now()), None);
        assert!(!session.is_open());
        assert_eq!(session.backend.closes, 1);
        assert_eq!(session.read(true, Instant::now()), Some(43.0));
        assert_eq!(session.backend.opens, 2);
    }

    #[test]
    fn retries_after_the_back_off_and_logs_both_ends() {
        let start = Instant::now();
        let mut session = Session::new(Fake {
            open_results: vec![Err(OpenError::Failed); 4],
            ..Fake::default()
        });
        for _ in 0..MAX_FAILURES {
            assert_eq!(session.read(true, start), None);
        }
        assert!(session.take_log().is_some_and(|line| line.contains("every 5 minutes")));
        // Inside the back-off nothing is attempted.
        assert_eq!(session.read(true, start + Duration::from_secs(60)), None);
        assert_eq!(session.backend.opens, MAX_FAILURES);
        // One retry when it ends; it fails, so another wait, no second line.
        let later = start + RETRY_AFTER;
        assert_eq!(session.read(true, later), None);
        assert_eq!(session.backend.opens, MAX_FAILURES + 1);
        assert_eq!(session.take_log(), None);
        assert_eq!(session.read(true, later + Duration::from_secs(1)), None);
        assert_eq!(session.backend.opens, MAX_FAILURES + 1);
        // The next retry works: readings again, and one line saying so.
        let recovered = later + RETRY_AFTER;
        assert_eq!(session.read(true, recovered), Some(50.0));
        assert!(session.take_log().is_some_and(|line| line.contains("again")));
        assert_eq!(session.read(true, recovered), Some(50.0));
    }

    #[test]
    fn a_missing_library_is_logged_once() {
        let mut session = Session::new(Fake {
            open_results: vec![Err(OpenError::Unavailable)],
            ..Fake::default()
        });
        assert_eq!(session.read(true, Instant::now()), None);
        assert!(session.take_log().is_some());
        assert_eq!(session.read(true, Instant::now()), None);
        assert_eq!(session.take_log(), None);
    }

    #[test]
    fn power_limits_come_in_watts_from_an_open_session_only() {
        let mut session = Session::new(Fake {
            limits: Some((1_000, 110_000)),
            ..Fake::default()
        });
        assert_eq!(session.power_limits(), None);
        session.read(true, Instant::now());
        assert_eq!(session.power_limits(), Some((1.0, 110.0)));
        session.backend.limits = Some((0, 0));
        assert_eq!(session.power_limits(), None);
        session.backend.limits = None;
        assert_eq!(session.power_limits(), None);
    }

    #[test]
    fn details_follow_the_same_rules_as_a_reading() {
        let mut session = Session::new(Fake {
            temperatures: vec![Some(47), None, Some(48)],
            ..Fake::default()
        });
        assert_eq!(session.details(false, Instant::now()), None);
        assert_eq!(
            session.backend.opens, 0,
            "never touches a GPU that may not be woken"
        );
        assert_eq!(
            session.details(true, Instant::now()).map(|d| d.temperature),
            Some(47)
        );
        // A failed one closes the session; the next opens it again.
        assert_eq!(session.details(true, Instant::now()), None);
        assert!(!session.is_open());
        assert_eq!(session.read(true, Instant::now()), Some(48.0));
        assert_eq!(session.backend.opens, 2);
    }
}
