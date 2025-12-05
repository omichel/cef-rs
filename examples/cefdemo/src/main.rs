use cef::{args::Args, rc::*, *};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

// V8 Handler for all Rust functions callable from JavaScript
wrap_v8_handler! {
    struct RustFunctionHandler;

    impl V8Handler {
        fn execute(
            &self,
            name: Option<&CefString>,
            _object: Option<&mut V8Value>,
            _arguments: Option<&[Option<V8Value>]>,
            retval: Option<&mut Option<V8Value>>,
            _exception: Option<&mut CefString>,
        ) -> ::std::os::raw::c_int {
            let Some(name) = name else { return 0 };
            let name_str = name.to_string();

            match name_str.as_str() {
                "helloWorld" => {
                    // Return "Hello World from Rust!" as a string
                    let result_str = CefString::from("Hello World from Rust!");
                    if let Some(retval) = retval {
                        *retval = v8_value_create_string(Some(&result_str));
                    }
                    1
                }
                "triggerCallback" => {
                    // Get the current V8 context to access the browser and frame
                    if let Some(context) = v8_context_get_current_context() {
                        if let Some(browser) = context.browser() {
                            if let Some(frame) = browser.main_frame() {
                                // Call JavaScript function from Rust!
                                let js_code = CefString::from(
                                    "onRustCallback('JavaScript function called from Rust.');"
                                );
                                let script_url = CefString::from("");
                                frame.execute_java_script(Some(&js_code), Some(&script_url), 0);
                            }
                        }
                    }
                    1
                }
                _ => 0, // Not handled
            }
        }
    }
}

// Render Process Handler to inject JavaScript bindings
wrap_render_process_handler! {
    struct DemoRenderProcessHandler;

    impl RenderProcessHandler {
        fn on_context_created(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            context: Option<&mut V8Context>,
        ) {
            if let Some(context) = context {
                // Get the global object (window)
                if let Some(global) = context.global() {
                    // Create a single handler for all our functions
                    let mut handler = RustFunctionHandler::new();

                    // Create a "rust" object to namespace our functions
                    if let Some(mut rust_obj) = v8_value_create_object(
                        Option::<&mut V8Accessor>::None,
                        Option::<&mut V8Interceptor>::None,
                    ) {
                        // Register all functions with the same handler
                        for func_name in ["helloWorld", "triggerCallback"] {
                            let name = CefString::from(func_name);
                            if let Some(mut func) =
                                v8_value_create_function(Some(&name), Some(&mut handler))
                            {
                                rust_obj.set_value_bykey(
                                    Some(&name),
                                    Some(&mut func),
                                    V8Propertyattribute::from(sys::cef_v8_propertyattribute_t::V8_PROPERTY_ATTRIBUTE_NONE),
                                );
                            }
                        }

                        // Add the rust object to the global scope (window.rust)
                        let rust_name = CefString::from("rust");
                        global.set_value_bykey(
                            Some(&rust_name),
                            Some(&mut rust_obj),
                            V8Propertyattribute::from(sys::cef_v8_propertyattribute_t::V8_PROPERTY_ATTRIBUTE_NONE),
                        );
                    }
                }
            }
        }
    }
}

/// Configuration for window state persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WindowConfig {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    maximized: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            x: 100,
            y: 100,
            width: 1024,
            height: 768,
            maximized: false,
        }
    }
}

impl WindowConfig {
    /// Get the config file path in the user's config directory
    fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("Cresus").join("window.json"))
    }

    /// Load window configuration from disk
    fn load() -> Self {
        Self::config_path()
            .and_then(|path| fs::read_to_string(&path).ok())
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    /// Save window configuration to disk
    fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(path) = Self::config_path() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let content = serde_json::to_string_pretty(self)?;
            fs::write(&path, content)?;
        }
        Ok(())
    }
}

wrap_app! {
    struct DemoApp {
        window: Arc<Mutex<Option<Window>>>,
        config: Arc<Mutex<WindowConfig>>,
    }

    impl App {
        fn on_before_command_line_processing(
            &self,
            _process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            if let Some(cmd) = command_line {
                // Disable Google Cloud Messaging and related features
                let switch = CefString::from("disable-background-networking");
                cmd.append_switch(Some(&switch));
            }
        }

        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(DemoBrowserProcessHandler::new(
                self.window.clone(),
                self.config.clone(),
            ))
        }

        fn render_process_handler(&self) -> Option<RenderProcessHandler> {
            // Return our custom render process handler that injects JavaScript bindings
            Some(DemoRenderProcessHandler::new())
        }
    }
}

wrap_browser_process_handler! {
    struct DemoBrowserProcessHandler {
        window: Arc<Mutex<Option<Window>>>,
        config: Arc<Mutex<WindowConfig>>,
    }

    impl BrowserProcessHandler {
        // The real lifespan of cef starts from `on_context_initialized`, so all the cef objects should be manipulated after that.
        fn on_context_initialized(&self) {
            let mut client = DemoClient::new();

            // Load HTML file from htdocs directory relative to executable
            let exe_path = std::env::current_exe().expect("Failed to get executable path");
            let exe_dir = exe_path.parent().expect("Failed to get executable directory");
            let html_path = exe_dir.join("htdocs").join("index.html");
            let url_string = format!("file:///{}", html_path.display().to_string().replace('\\', "/"));
            let url = CefString::from(url_string.as_str());

            let browser_view = browser_view_create(
                Some(&mut client),
                Some(&url),
                Some(&Default::default()),
                Option::<&mut DictionaryValue>::None,
                Option::<&mut RequestContext>::None,
                Option::<&mut BrowserViewDelegate>::None,
            )
            .expect("Failed to create browser view");

            let mut delegate = DemoWindowDelegate::new(browser_view, self.config.clone());
            if let Ok(mut window) = self.window.lock() {
                *window = Some(
                    window_create_top_level(Some(&mut delegate)).expect("Failed to create window"),
                );
            }
        }
    }
}

// Keyboard handler to block F12 and Ctrl+Shift+I (DevTools) in release mode
#[cfg(not(debug_assertions))]
wrap_keyboard_handler! {
    struct DemoKeyboardHandler;

    impl KeyboardHandler {
        fn on_pre_key_event(
            &self,
            _browser: Option<&mut Browser>,
            event: Option<&KeyEvent>,
            _os_event: Option<&mut sys::MSG>,
            is_keyboard_shortcut: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            if let Some(event) = event {
                // Block F12 key (windows_key_code 123)
                if event.windows_key_code == 123 {
                    if let Some(shortcut) = is_keyboard_shortcut {
                        *shortcut = 0;
                    }
                    return 1; // Event handled, suppress it
                }
                // Block Ctrl+Shift+I (windows_key_code 73 = 'I')
                // EVENTFLAG_SHIFT_DOWN = 2
                // EVENTFLAG_CONTROL_DOWN = 4
                const CTRL_SHIFT: u32 = 4 | 2;
                if event.windows_key_code == 73 && (event.modifiers & CTRL_SHIFT) == CTRL_SHIFT {
                    if let Some(shortcut) = is_keyboard_shortcut {
                        *shortcut = 0;
                    }
                    return 1; // Event handled, suppress it
                }
            }
            0 // Let other keys pass through
        }
    }
}

// Context menu handler to disable right-click menu in release mode
#[cfg(not(debug_assertions))]
wrap_context_menu_handler! {
    struct DemoContextMenuHandler;

    impl ContextMenuHandler {
        fn on_before_context_menu(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _params: Option<&mut ContextMenuParams>,
            model: Option<&mut MenuModel>,
        ) {
            // Clear the context menu to disable it entirely
            if let Some(model) = model {
                model.clear();
            }
        }
    }
}

wrap_client! {
    struct DemoClient;

    impl Client {
        #[cfg(not(debug_assertions))]
        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            Some(DemoKeyboardHandler::new())
        }

        #[cfg(not(debug_assertions))]
        fn context_menu_handler(&self) -> Option<ContextMenuHandler> {
            Some(DemoContextMenuHandler::new())
        }
    }
}

wrap_window_delegate! {
    struct DemoWindowDelegate {
        browser_view: BrowserView,
        config: Arc<Mutex<WindowConfig>>,
    }

    impl ViewDelegate {
        fn on_child_view_changed(
            &self,
            _view: Option<&mut View>,
            _added: ::std::os::raw::c_int,
            _child: Option<&mut View>,
        ) {
            // view.as_panel().map(|x| x.as_window().map(|w| w.close()));
        }
    }

    impl PanelDelegate {}

    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            if let Some(window) = window {
                let view = self.browser_view.clone();
                window.add_child_view(Some(&mut (&view).into()));

                // Load and set the window icons
                let exe_path = std::env::current_exe().expect("Failed to get executable path");
                let exe_dir = exe_path.parent().expect("Failed to get executable directory");
                let icon_path = exe_dir.join("icons").join("icon.png");

                if let Ok(icon_data) = std::fs::read(&icon_path) {
                    // Set window icon (title bar)
                    if let Some(mut image) = image_create() {
                        if image.add_png(1.0, Some(&icon_data)) != 0 {
                            window.set_window_icon(Some(&mut image));
                        }
                    }
                    // Set app icon (taskbar)
                    if let Some(mut image) = image_create() {
                        if image.add_png(1.0, Some(&icon_data)) != 0 {
                            window.set_window_app_icon(Some(&mut image));
                        }
                    }
                }

                // Set window title
                let title = CefString::from("Crésus");
                window.set_title(Some(&title));

                // Restore window position and size from saved config
                if let Ok(config) = self.config.lock() {
                    let bounds = Rect {
                        x: config.x,
                        y: config.y,
                        width: config.width,
                        height: config.height,
                    };
                    window.set_bounds(Some(&bounds));

                    window.show();

                    // Restore maximized state after showing the window
                    if config.maximized {
                        window.maximize();
                    }
                } else {
                    window.show();
                }
            }
        }

        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            quit_message_loop();
        }

        fn with_standard_window_buttons(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            1
        }

        fn can_resize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            1
        }

        fn can_maximize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            1
        }

        fn can_minimize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            1
        }

        fn can_close(&self, window: Option<&mut Window>) -> ::std::os::raw::c_int {
            // Save window position and size before closing (while window is still valid)
            if let Some(window) = window {
                if let Ok(mut config) = self.config.lock() {
                    let is_maximized = window.is_maximized() != 0;
                    config.maximized = is_maximized;

                    // Only save bounds if not maximized (to preserve the restore size)
                    if !is_maximized {
                        let bounds = window.bounds_in_screen();
                        config.x = bounds.x;
                        config.y = bounds.y;
                        config.width = bounds.width;
                        config.height = bounds.height;
                    }

                    if let Err(e) = config.save() {
                        eprintln!("Failed to save window config: {}", e);
                    }
                }
            }
            1
        }
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    let _loader = {
        let loader = library_loader::LibraryLoader::new(&std::env::current_exe().unwrap(), false);
        assert!(loader.load());
        loader
    };

    #[cfg(target_os = "macos")]
    {
        use objc2::{
            ClassType, MainThreadMarker, msg_send,
            rc::Retained,
            runtime::{AnyObject, NSObjectProtocol},
        };
        use objc2_app_kit::NSApp;

        use application::SimpleApplication;

        let mtm = MainThreadMarker::new().unwrap();

        unsafe {
            // Initialize the SimpleApplication instance.
            // SAFETY: mtm ensures that here is the main thread.
            let _: Retained<AnyObject> = msg_send![SimpleApplication::class(), sharedApplication];
        }

        // If there was an invocation to NSApp prior to here,
        // then the NSApp will not be a SimpleApplication.
        // The following assertion ensures that this doesn't happen.
        assert!(NSApp(mtm).isKindOfClass(SimpleApplication::class()));
    }

    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

    let args = Args::new();
    let cmd = args.as_cmd_line().unwrap();

    let switch = CefString::from("type");
    let is_browser_process = cmd.has_switch(Some(&switch)) != 1;

    let window = Arc::new(Mutex::new(None));
    let config = Arc::new(Mutex::new(WindowConfig::load()));
    let mut app = DemoApp::new(window.clone(), config);

    let ret = execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );

    if is_browser_process {
        assert!(ret == -1, "cannot execute browser process");
    } else {
        assert!(ret >= 0, "cannot execute non-browser process");
        // non-browser process does not initialize cef
        return;
    }
    let settings = Settings {
        no_sandbox: !cfg!(feature = "sandbox") as _,
        log_severity: LogSeverity::from(sys::cef_log_severity_t::LOGSEVERITY_FATAL),
        ..Default::default()
    };
    assert_eq!(
        initialize(
            Some(args.as_main_args()),
            Some(&settings),
            Some(&mut app),
            std::ptr::null_mut(),
        ),
        1
    );

    run_message_loop();

    let window = window.lock().expect("Failed to lock window");
    let window = window.as_ref().expect("Window is None");
    assert!(window.has_one_ref());

    shutdown();
}

#[cfg(target_os = "macos")]
mod application {
    use std::cell::Cell;

    use cef::application_mac::{CefAppProtocol, CrAppControlProtocol, CrAppProtocol};
    use objc2::{DefinedClass, define_class, runtime::Bool};
    use objc2_app_kit::NSApplication;

    /// Instance variables of `SimpleApplication`.
    pub struct SimpleApplicationIvars {
        handling_send_event: Cell<Bool>,
    }

    define_class!(
        /// A `NSApplication` subclass that implements the required CEF protocols.
        ///
        /// This class provides the necessary `CefAppProtocol` conformance to
        /// ensure that events are handled correctly by the Chromium framework on macOS.
        #[unsafe(super(NSApplication))]
        #[ivars = SimpleApplicationIvars]
        pub struct SimpleApplication;

        unsafe impl CrAppControlProtocol for SimpleApplication {
            #[unsafe(method(setHandlingSendEvent:))]
            unsafe fn set_handling_send_event(&self, handling_send_event: Bool) {
                self.ivars().handling_send_event.set(handling_send_event);
            }
        }

        unsafe impl CrAppProtocol for SimpleApplication {
            #[unsafe(method(isHandlingSendEvent))]
            unsafe fn is_handling_send_event(&self) -> Bool {
                self.ivars().handling_send_event.get()
            }
        }

        unsafe impl CefAppProtocol for SimpleApplication {}
    );
}
