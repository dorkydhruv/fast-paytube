use fast_core::{ base_types::*, error::*, message::*, serialization::* };
use log::{ error, info };
use std::net::SocketAddr;
use tokio::net::UdpSocket;

const DEFAULT_BUFFER_SIZE: usize = 65536;

/// UDP client for communicating with authority shards
pub struct UdpClient {
    socket: UdpSocket,
    buffer_size: usize,
}

impl UdpClient {
    pub async fn new() -> Result<Self, std::io::Error> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        Ok(Self {
            socket,
            buffer_size: DEFAULT_BUFFER_SIZE,
        })
    }

    /// Send a message and receive a response
    pub async fn send_recv(
        &self,
        addr: SocketAddr,
        data: Vec<u8>
    ) -> Result<Vec<u8>, FastPayError> {
        // Send the data
        self.socket.send_to(&data, addr).await.map_err(|_| FastPayError::CommunicationError)?;

        // Receive the response
        let mut buffer = vec![0; self.buffer_size];
        let (len, _) = self.socket
            .recv_from(&mut buffer).await
            .map_err(|_| FastPayError::CommunicationError)?;

        Ok(buffer[..len].to_vec())
    }
}

/// UDP server for handling authority requests
pub struct UdpServer {
    socket: UdpSocket,
    buffer_size: usize,
}

impl UdpServer {
    pub async fn new(addr: SocketAddr) -> Result<Self, std::io::Error> {
        let socket = UdpSocket::bind(addr).await?;
        info!("Server listening on {}", addr);
        Ok(Self {
            socket,
            buffer_size: DEFAULT_BUFFER_SIZE,
        })
    }

    /// Start the server and process incoming messages
    pub async fn run<F>(&self, handler: F) -> Result<(), std::io::Error>
        where F: Fn(&[u8]) -> Option<Vec<u8>> + Send + Sync + 'static
    {
        let mut buffer = vec![0; self.buffer_size];

        loop {
            match self.socket.recv_from(&mut buffer).await {
                Ok((len, addr)) => {
                    let data = &buffer[..len];
                    if let Some(response) = handler(data) {
                        if let Err(e) = self.socket.send_to(&response, addr).await {
                            error!("Failed to send response: {}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to receive data: {}", e);
                }
            }
        }
    }
}

/// Authority client for a specific shard
pub struct AuthorityShardClient {
    client: UdpClient,
    address: SocketAddr,
    _authority: AuthorityName,
}

impl AuthorityShardClient {
    pub async fn new(
        _authority: AuthorityName,
        address: SocketAddr
    ) -> Result<Self, std::io::Error> {
        let client = UdpClient::new().await?;

        Ok(Self {
            client,
            address,
            _authority,
        })
    }

    /// Send a transfer order to the authority
    pub async fn send_transfer_order(
        &self,
        order: &CrossChainTransferOrder
    ) -> Result<SignedCrossChainTransferOrder, FastPayError> {
        // Serialize and send the order
        let request = serialize_transfer_order(order);
        let response_bytes = self.client.send_recv(self.address, request).await?;

        // Deserialize the response
        match deserialize_message(&response_bytes)? {
            BridgeMessage::SignedCrossChainTransferOrder(signed_order) => Ok(signed_order),
            BridgeMessage::Error(error) => {
                error!("Authority returned error: {}", error);
                Err(FastPayError::CommunicationError)
            }
            _ => {
                error!("Unexpected response from authority");
                Err(FastPayError::CommunicationError)
            }
        }
    }

    /// Send a certified order to the authority
    pub async fn send_certified_order(
        &self,
        order: &CertifiedCrossChainTransferOrder
    ) -> Result<(), FastPayError> {
        // Serialize and send the order
        let request = serialize_certified_order(order);
        let _ = self.client.send_recv(self.address, request).await?;

        // We don't expect a response for this message
        Ok(())
    }
}
