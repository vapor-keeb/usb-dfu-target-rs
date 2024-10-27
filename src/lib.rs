#![no_std]

use consts::{DfuRequest, DfuStatus};
#[cfg(feature = "defmt")]
use defmt::{debug, error, info};

pub mod consts;

/// Internal State used to keep track of DFU State
enum InternalState {
    Idle,
    DownloadBusy(DownloadState),
    Download(DownloadState),
    Upload { offset: usize },
    Error,
}

impl Into<consts::State> for &InternalState {
    fn into(self) -> consts::State {
        use consts::State;

        match self {
            InternalState::Idle => State::DfuIdle,
            InternalState::DownloadBusy(_) => State::DownloadBusy,
            InternalState::Download(_) => State::DownloadIdle,
            InternalState::Upload { .. } => State::UploadIdle,
            InternalState::Error => State::Error,
        }
    }
}

/// Internal state for download offset bookkeeping
struct DownloadState {
    offset: usize,
}

/// This is the trait the user of this library should implement
/// to handle the specifics of the hardware programming.
/// i.e. writing to flash
pub trait DfuHandler {
    /// This function should not block for extended period of time
    fn write_data(&mut self, offset: usize, data: &[u8]);
    /// Return if the previous write is done
    fn is_write_complete(&self) -> bool {
        true
    }

    /// Callback to complete a download request.
    /// Corresponds to the manifest stage of USB DFU.
    fn complete_download(&mut self);

    /// Read bytes into buffer from offset, return the number of bytes read.
    fn upload(&self, buffer: &mut [u8], offset: usize) -> usize;
}

/// USB Device in DFU mode
pub struct UsbDfuDevice<'a> {
    state: InternalState,
    handler: &'a mut dyn DfuHandler,
}

impl<'a> UsbDfuDevice<'a> {
    pub fn new(handler: &'a mut dyn DfuHandler) -> Self {
        UsbDfuDevice {
            state: InternalState::Idle,
            handler,
        }
    }

    fn handle_download(
        &mut self,
        DownloadState { offset }: DownloadState,
        buf: &[u8],
    ) -> (InternalState, Result<(), ()>) {
        if buf.len() == 0 {
            return (InternalState::Idle, Ok(()));
        }

        self.handler.write_data(offset, buf);
        return (
            InternalState::DownloadBusy(DownloadState {
                offset: offset + buf.len(),
            }),
            Ok(()),
        );
    }

    pub fn handle_control_out(&mut self, req: DfuRequest, data: &[u8]) -> Result<(), ()> {
        let (state, response) = match core::mem::replace(&mut self.state, InternalState::Error) {
            InternalState::Idle => match req {
                DfuRequest::Dnload => self.handle_download(DownloadState { offset: 0 }, data),
                DfuRequest::Abort => (InternalState::Idle, Ok(())),
                r => {
                    #[cfg(feature = "defmt")]
                    error!("received {} at state idle", r as u8);
                    // TODO: set this to error state
                    (InternalState::Idle, Err(()))
                }
            },
            InternalState::Download(download_state) => match req {
                DfuRequest::Dnload => self.handle_download(download_state, data),
                DfuRequest::Abort => (InternalState::Idle, Ok(())),
                r => {
                    #[cfg(feature = "defmt")]
                    error!("received {} at state download", r as u8);
                    (InternalState::Error, Err(()))
                }
            },
            state => {
                #[cfg(feature = "defmt")]
                error!("received {} for control out", req as u8);

                (state, Err(()))
            }
        };
        self.state = state;
        response
    }

    pub fn handle_control_in<'b>(
        &mut self,
        req: DfuRequest,
        buf: &'b mut [u8],
    ) -> Result<&'b [u8], ()> {
        match req {
            DfuRequest::GetState => {
                buf[0] = Into::<consts::State>::into(&self.state) as u8;
                Ok(&buf[0..1])
            }
            DfuRequest::GetStatus => {
                let (next_state, status) =
                    match core::mem::replace(&mut self.state, InternalState::Error) {
                        InternalState::DownloadBusy(download_state) => {
                            match self.handler.is_write_complete() {
                                true => (InternalState::Download(download_state), DfuStatus::Ok),
                                false => {
                                    (InternalState::DownloadBusy(download_state), DfuStatus::Ok)
                                }
                            }
                        }
                        InternalState::Error => (InternalState::Error, DfuStatus::ErrUnknown),
                        state => (state, DfuStatus::Ok),
                    };
                self.state = next_state;
                let next_state = Into::<consts::State>::into(&self.state);
                buf[0] = status as u8; // bStatus

                // This is cursed.... basically the highest byte is overriden by the next line
                buf[1..5].copy_from_slice(&(250_u32.to_le_bytes())); // [1..4] bwPoolTimeout
                buf[4] = next_state as u8; // bstate
                buf[5] = 0; // iString

                Ok(&buf[0..6])
            }
            req => {
                let (state, response): (InternalState, Result<&'a [u8], ()>) =
                    match core::mem::replace(&mut self.state, InternalState::Error) {
                        InternalState::Idle => todo!(),
                        InternalState::DownloadBusy(download_state) => todo!(),
                        InternalState::Download(download_state) => todo!(),
                        InternalState::Upload { offset } => todo!(),
                        InternalState::Error => todo!(),
                    };
            }
        }
    }
}
