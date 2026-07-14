use std::io::BufRead;

use crate::{
    Result,
    models::{ADBCommand, ADBHostCommand},
    server::{ADBServer, MDNSServices, models::MDNSBackend},
};

const OPENSCREEN_MDNS_BACKEND: &str = "ADB_MDNS_OPENSCREEN";

impl ADBServer {
    /// Check if mdns discovery is available
    pub fn mdns_check(&mut self) -> Result<bool> {
        let response = self
            .connect()?
            .proxy_connection(&ADBCommand::Host(ADBHostCommand::MDNSCheck), true)?;

        match String::from_utf8(response) {
            Ok(s) if s.starts_with("mdns daemon version") => Ok(true),
            Ok(_) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// List all discovered mdns services
    pub fn mdns_services(&mut self) -> Result<Vec<MDNSServices>> {
        let services = self
            .connect()?
            .proxy_connection(&ADBCommand::Host(ADBHostCommand::MDNSServices), true)?;

        let mut vec_services: Vec<MDNSServices> = vec![];
        for service in services.lines() {
            match service {
                Ok(service) => {
                    vec_services.push(MDNSServices::try_from(service.as_bytes())?);
                }
                Err(e) => log::error!("{e}"),
            }
        }

        Ok(vec_services)
    }

    /// Check if specified backend mdns service is used, otherwise restart adb server with envs
    pub fn mdns_force_backend(&mut self, backend: MDNSBackend) -> Result<()> {
        let server_status = self.server_status()?;
        if server_status.mdns_backend != backend {
            self.kill()?;
            self.envs.insert(
                OPENSCREEN_MDNS_BACKEND.to_string(),
                (if backend == MDNSBackend::OpenScreen {
                    "1"
                } else {
                    "0"
                })
                .to_string(),
            );
            self.connect()?;
        }

        Ok(())
    }
}
