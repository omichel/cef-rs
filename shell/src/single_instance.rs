//! Single instance support for Windows
//!
//! Uses a named mutex for detection and a named pipe for IPC communication
//! between instances.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, ERROR_PIPE_CONNECTED, GENERIC_READ, GENERIC_WRITE,
    GetLastError, HANDLE, INVALID_HANDLE_VALUE,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileA, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_NONE, FlushFileBuffers, OPEN_EXISTING,
    PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeA, DisconnectNamedPipe, PIPE_READMODE_MESSAGE,
    PIPE_TYPE_MESSAGE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows::Win32::System::Threading::{CreateMutexA, ReleaseMutex};
use windows::core::{PCSTR, s};

const MUTEX_NAME: PCSTR = s!("Global\\CresusAppSingleInstanceMutex");
const PIPE_NAME: PCSTR = s!("\\\\.\\pipe\\CresusAppSingleInstancePipe");
const BUFFER_SIZE: u32 = 4096;

/// Represents the single instance manager
pub struct SingleInstance {
    mutex_handle: HANDLE,
    is_first_instance: bool,
    message_receiver: Option<Receiver<String>>,
    _listener_thread: Option<thread::JoinHandle<()>>,
}

impl SingleInstance {
    /// Create a new single instance manager.
    /// Returns the manager with information about whether this is the first instance.
    pub fn new() -> Self {
        let (mutex_handle, is_first_instance) = unsafe { Self::try_create_mutex() };

        let (message_receiver, listener_thread) = if is_first_instance {
            // Start pipe listener for incoming messages from other instances
            let (tx, rx) = mpsc::channel();
            let handle = thread::spawn(move || {
                Self::pipe_listener(tx);
            });
            (Some(rx), Some(handle))
        } else {
            (None, None)
        };

        Self {
            mutex_handle,
            is_first_instance,
            message_receiver,
            _listener_thread: listener_thread,
        }
    }

    /// Check if this is the first instance
    pub fn is_first_instance(&self) -> bool {
        self.is_first_instance
    }

    /// Take ownership of the message receiver channel (only available for first instance)
    /// This consumes the receiver, so it can only be called once.
    pub fn take_message_receiver(&mut self) -> Option<Receiver<String>> {
        self.message_receiver.take()
    }

    /// Send a message to the first instance (called by subsequent instances)
    pub fn send_to_first_instance(message: &str) -> bool {
        unsafe {
            // Try to connect to the named pipe
            let pipe_handle = CreateFileA(
                PIPE_NAME,
                (GENERIC_READ.0 | GENERIC_WRITE.0).into(),
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            );

            let pipe_handle = match pipe_handle {
                Ok(h) => h,
                Err(_) => return false,
            };

            if pipe_handle == INVALID_HANDLE_VALUE {
                return false;
            }

            // Write the message
            let message_bytes = message.as_bytes();
            let mut bytes_written = 0u32;
            let success = WriteFile(
                pipe_handle,
                Some(message_bytes),
                Some(&mut bytes_written),
                None,
            );

            let _ = FlushFileBuffers(pipe_handle);
            let _ = CloseHandle(pipe_handle);

            success.is_ok()
        }
    }

    /// Try to create the mutex, returns (handle, is_first_instance)
    unsafe fn try_create_mutex() -> (HANDLE, bool) {
        unsafe {
            let handle = CreateMutexA(None, false, MUTEX_NAME);

            match handle {
                Ok(h) => {
                    let is_first = GetLastError() != ERROR_ALREADY_EXISTS;
                    (h, is_first)
                }
                Err(_) => (HANDLE::default(), false),
            }
        }
    }

    /// Pipe listener that runs in a background thread
    fn pipe_listener(sender: Sender<String>) {
        loop {
            unsafe {
                // Create named pipe
                let pipe_handle = CreateNamedPipeA(
                    PIPE_NAME,
                    PIPE_ACCESS_DUPLEX,
                    PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
                    PIPE_UNLIMITED_INSTANCES,
                    BUFFER_SIZE,
                    BUFFER_SIZE,
                    0,
                    None,
                );

                let pipe_handle = match pipe_handle {
                    Ok(h) => h,
                    Err(_) => {
                        thread::sleep(std::time::Duration::from_millis(100));
                        continue;
                    }
                };

                if pipe_handle == INVALID_HANDLE_VALUE {
                    thread::sleep(std::time::Duration::from_millis(100));
                    continue;
                }

                // Wait for a client to connect
                let connected = ConnectNamedPipe(pipe_handle, None);

                // ConnectNamedPipe returns false if client connected between CreateNamedPipe and ConnectNamedPipe
                // In that case, GetLastError returns ERROR_PIPE_CONNECTED which is still a valid connection
                if connected.is_err() && GetLastError() != ERROR_PIPE_CONNECTED {
                    let _ = CloseHandle(pipe_handle);
                    continue;
                }

                // Read the message
                let mut buffer = vec![0u8; BUFFER_SIZE as usize];
                let mut bytes_read = 0u32;
                let read_result =
                    ReadFile(pipe_handle, Some(&mut buffer), Some(&mut bytes_read), None);

                if read_result.is_ok() && bytes_read > 0 {
                    if let Ok(message) = String::from_utf8(buffer[..bytes_read as usize].to_vec()) {
                        let _ = sender.send(message);
                    }
                }

                // Disconnect and close
                let _ = DisconnectNamedPipe(pipe_handle);
                let _ = CloseHandle(pipe_handle);
            }
        }
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        if !self.mutex_handle.is_invalid() {
            unsafe {
                let _ = ReleaseMutex(self.mutex_handle);
                let _ = CloseHandle(self.mutex_handle);
            }
        }
    }
}

/// Get the command line arguments as a JSON string for sending to first instance
pub fn get_args_json() -> String {
    let args: Vec<String> = std::env::args().collect();
    serde_json::to_string(&args).unwrap_or_else(|_| "[]".to_string())
}
