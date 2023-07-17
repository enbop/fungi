#![feature(async_fn_in_trait)]

use std::{error, fmt};

#[derive(Debug)]
enum ServerError {
    AlreadyStarted,
    NotStarted,
    AlreadyStopped,
    NotStopped,
    Unknown,
    Other(String),
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerError::AlreadyStarted => write!(f, "Server already started"),
            ServerError::NotStarted => write!(f, "Server not started"),
            ServerError::AlreadyStopped => write!(f, "Server already stopped"),
            ServerError::NotStopped => write!(f, "Server not stopped"),
            ServerError::Unknown => write!(f, "Unknown error"),
            ServerError::Other(s) => write!(f, "{}", s),
        }
    }
}

impl error::Error for ServerError {}

trait Verifier {
    async fn verify<ID>(&self, identity: ID) -> bool; // TODO Error type, such as TooManyAttempts
}

trait ServerAbstract {
    type VerifierImpl: Verifier;

    async fn start(&self) -> Result<(), ServerError>;
    async fn stop(&self) -> Result<(), ServerError>;

    fn get_verifier(&self) -> &Self::VerifierImpl;

    async fn verify<ID>(&self, identity: ID) -> Result<bool, ServerError> {
        Ok(self.get_verifier().verify(identity).await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use server_protobuf::server::Person;

    #[test]
    fn it_works() {
        let mut server = Person::default();
        server.name = "test".to_string();
        assert_eq!(server.name, "test");
    }
}
