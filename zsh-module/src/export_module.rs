use std::{
    ffi::{c_char, c_int, CStr},
    sync::atomic::AtomicBool,
};

use crate::{log, options::Opts, to_cstr, AnyError, Module};

use parking_lot::Mutex;
use zsh_sys as zsys;

struct ModuleHolder {
    module: Mutex<Option<Module>>,
    panicked: AtomicBool,
}

impl ModuleHolder {
    const fn empty() -> Self {
        Self {
            module: parking_lot::const_mutex(None),
            panicked: AtomicBool::new(false),
        }
    }
}

// This struct is neither of them, but since it isn't exposed to user code
// and it isn't given to any threads, this should be safe.
unsafe impl Sync for ModuleHolder {}
unsafe impl Send for ModuleHolder {}

static MODULE: ModuleHolder = ModuleHolder::empty();

unsafe fn strings_from_ptr<'a>(mut ptr: *const *const c_char) -> Vec<&'a str> {
    let mut vec = Vec::with_capacity(2);
    loop {
        if (*ptr).is_null() {
            break vec;
        }
        vec.push(CStr::from_ptr(*ptr).to_str().expect("Failed to parse arg"));
        ptr = ptr.add(1);
    }
}

extern "C" fn builtin_callback(
    name: *mut c_char,
    args: *mut *mut c_char,
    opts: *mut zsys::options,
    _: i32,
) -> i32 {
    handle_panic(name, || {
        let args = unsafe { strings_from_ptr(std::mem::transmute(args)) };
        let name = unsafe { CStr::from_ptr(name) };
        let opts = unsafe { Opts::from_raw(opts) };

        let mut module = get_mod();
        let Module {
            bintable,
            user_data,
            ..
        } = &mut *module;
        let bin = bintable.get_mut(name).expect("Failed to find binary name");
        match bin(
            &mut **user_data,
            name.to_str().expect("Failed to parse binary name"),
            &args,
            opts,
        ) {
            Ok(()) => 0,
            Err(e) => {
                let msg = to_cstr(e.to_string());
                log::error_named(name, msg);
                1
            }
        }
    })
    .unwrap_or(65)
}

fn set_mod(module: Module) {
    *MODULE.module.lock() = Some(module);
}

fn drop_mod() {
    if !panicked() {
        MODULE.module.lock().take();
    }
}

fn get_mod() -> parking_lot::MappedMutexGuard<'static, Module> {
    parking_lot::MutexGuard::map(MODULE.module.lock(), |opt| {
        opt.as_mut().expect("No module set")
    })
}

unsafe fn mod_get_name<'a>(module: zsys::Module) -> &'a CStr {
    CStr::from_ptr((*module).node.nam)
}

fn panicked() -> bool {
    MODULE.panicked.load(std::sync::atomic::Ordering::Acquire)
}

fn handle_panic<F, N, R>(name: N, cb: F) -> Option<R>
where
    F: FnOnce() -> R + std::panic::UnwindSafe,
    N: std::fmt::Debug,
{
    let res = std::panic::catch_unwind(|| cb());
    match res {
        Ok(ret) => Some(ret),
        Err(err) => {
            MODULE
                .panicked
                .store(true, std::sync::atomic::Ordering::Release);
            if let Some(msg) = err.downcast_ref::<&str>() {
                crate::error!("{:?} Panic: {}", name, msg);
            } else if let Some(msg) = err.downcast_ref::<String>() {
                crate::error!("{:?} Panic: {}", name, msg);
            } else {
                crate::error!("{:?} Panic: No additional information", name);
            }
            None
        }
    }
}

extern "Rust" {
    fn __zsh_rust_setup() -> Result<Module, AnyError>;
}

#[macro_export]
/// Exports a `setup` function to be called when the module needs to be set up.
macro_rules! export_module {
    ($name:ident) => {
        #[no_mangle]
        #[doc(hidden)]
        fn __zsh_rust_setup() -> ::std::result::Result<$crate::Module, Box<dyn ::std::error::Error>>
        {
            $name().map_err(::std::boxed::Box::from)
        }
    };
}

macro_rules! mod_fn {
    (fn $name:ident($mod:ident $(,$arg:ident : $type:ty)*) try $block:expr) => {
        mod_fn!(
            fn $name($mod $(,$arg : $type)*) {
                match $block {
                    Ok(()) =>  0,
                    Err(e) => { $crate::error!("{:?}: {}", unsafe { mod_get_name($mod) }, e); 1 },
                }
            }
        );
    };
    (fn $name:ident($mod:ident $(,$arg:ident : $type:ty)*) $block:expr) => {
        #[no_mangle]
        extern "C" fn $name($mod: $crate::zsys::Module $(,$arg: $type)*) -> i32 {
            handle_panic(unsafe { mod_get_name($mod) }.to_str().unwrap(), || {
                $block
            }).unwrap_or(65)
        }
    };
}

mod_fn!(
    fn setup_(_mod) {
        let mut module = match unsafe { __zsh_rust_setup() } {
            Ok(module) => module,
            Err(e) => {
                crate::error!("Failed to setup module: {}", e);
                return 1
            }
        };
        for x in module.features.get_binaries() {
            x.handlerfunc = Some(builtin_callback)
        }
        set_mod(module);
        0
    }
);

mod_fn!(
    fn boot_(_mod) try {
        // zsys::addwrapper()
        Ok::<_, std::convert::Infallible>(())
    }
);

mod_fn!(
    fn features_(mod_, features_ptr: *mut *mut *mut c_char) {
        let mut module = get_mod();
        unsafe { *features_ptr = zsys::featuresarray(mod_, &mut *module.features) };
        0
    }
);

mod_fn!(
    fn enables_(mod_, enables_ptr: *mut *mut c_int) {
        let mut module = get_mod();
        unsafe {
            zsys::handlefeatures(mod_, &mut *module.features, enables_ptr)
        }
    }
);

// Called when cleaning the module up.
mod_fn!(
    fn cleanup_(_mod) {
        let mut module = get_mod();
        unsafe {
            zsys::setfeatureenables(_mod, &mut *module.features, std::ptr::null_mut())
        }
    }
);

// Called after cleanup and when module fails to load.
mod_fn!(
    fn finish_(_mod) try {
        drop_mod();
        Ok::<(), std::convert::Infallible>(())
    }
);
