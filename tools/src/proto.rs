use std::io;

use once_cell::sync::Lazy;
use prost::{encoding::decode_varint, Message};
use regex::bytes::Regex as BytesRegex;

use crate::serial::{SerialCli, SerialReader};

#[rustfmt::skip]
pub mod pb;

#[rustfmt::skip]
pub mod pb_app;

#[rustfmt::skip]
pub mod pb_desktop;

#[rustfmt::skip]
pub mod pb_gpio;

#[rustfmt::skip]
pub mod pb_gui;

#[rustfmt::skip]
pub mod pb_property;

#[rustfmt::skip]
pub mod pb_storage;

#[rustfmt::skip]
pub mod pb_system;

pub static VARINT_END: Lazy<BytesRegex> = Lazy::new(|| BytesRegex::new(r"[\x00-\x7F]").unwrap());

pub struct RpcSession {
    serial: SerialReader,
    buf: Vec<u8>,
}

impl RpcSession {
    pub(crate) fn from_cli(serial: SerialReader) -> io::Result<Self> {
        Ok(Self {
            serial,
            buf: Vec::with_capacity(1024),
        })
    }

    /// Sends a message to the Flipper Zero.
    fn send(&mut self, command_id: u32, req: impl Into<pb::main::Content>) -> io::Result<()> {
        // Construct the request message.
        let mut msg = pb::Main::default();
        msg.command_id = command_id;
        msg.set_command_status(pb::CommandStatus::Ok);
        msg.has_next = false;
        msg.content = Some(req.into());

        // Send the request.
        msg.encode_length_delimited(&mut self.buf)?;
        self.serial.get_mut().write_all(&self.buf)?;
        self.buf.clear();

        Ok(())
    }

    /// Reads a single message from the Flipper Zero.
    fn receive(&mut self) -> io::Result<pb::Main> {
        // Read the length prefix of the response.
        let mut length_prefix = self.serial.read_until(&VARINT_END, false)?;
        self.buf.extend_from_slice(&length_prefix);
        let length = decode_varint(&mut length_prefix)?;

        // Read the response.
        let data = self.serial.read_exact(length as usize)?;
        self.buf.extend_from_slice(&data);
        let response = pb::Main::decode_length_delimited(&self.buf[..])?;
        self.buf.clear();

        match response.command_status() {
            pb::CommandStatus::Ok => Ok(response),
            error => Err(io::Error::new(io::ErrorKind::Other, format!("{:?}", error))),
        }
    }

    /// Sends a request to the Flipper Zero, expecting a single response.
    pub fn request<T>(
        &mut self,
        command_id: u32,
        req: pb::main::Content,
        f: impl FnOnce(pb::main::Content) -> Result<T, pb::main::Content>,
    ) -> io::Result<T> {
        self.send(command_id, req)?;

        let resp = self.receive()?;
        if resp.has_next {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Request generated more than one response"),
            ));
        }

        let content = resp.content.expect("should have content");
        f(content).map_err(|r| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Unexpected response: {:?}", r),
            )
        })
    }

    /// Sends a request to the Flipper Zero, accumulating multiple responses.
    pub fn request_many(
        &mut self,
        command_id: u32,
        req: impl Into<pb::main::Content>,
        mut acc: impl FnMut(pb::main::Content) -> io::Result<Result<(), pb::main::Content>>,
    ) -> io::Result<()> {
        self.send(command_id, req)?;

        let mut resp = self.receive()?;
        while resp.has_next {
            let content = resp.content.take().expect("has content");
            acc(content)?.map_err(|r| {
                io::Error::new(
                    io::ErrorKind::Other,
                    format!("Unexpected response: {:?}", r),
                )
            })?;
            resp = self.receive()?;
        }
        let content = resp.content.take().expect("has content");
        acc(content)?.map_err(|r| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Unexpected response: {:?}", r),
            )
        })?;

        Ok(())
    }

    /// Stops the Protobuf RPC session, returning to the text-based CLI.
    pub fn stop_session(mut self) -> io::Result<SerialCli> {
        self.send(0, pb::main::Content::StopSession(pb::StopSession {}))?;
        Ok(SerialCli::from_reader(self.serial))
    }
}
