use anyhow::Result;
use async_trait::async_trait;
use candle_core::{Device, Tensor};
use tokio::net::TcpStream;

use crate::model::Cache;

use super::{Message, WorkerInfo};

#[derive(Debug)]
pub struct Client {
    device: Device,
    address: String,
    layer_name: String,
    stream: TcpStream,
    worker_info: WorkerInfo,
}

impl Client {
    pub async fn new(device: Device, address: &str, layer_name: &str) -> Result<Self> {
        let address = address.to_string();
        let layer_name = layer_name.to_string();
        let stream = TcpStream::connect(&address).await?;
        let worker_info = WorkerInfo {
            device: String::new(),
        };

        let mut client = Self {
            address,
            device,
            stream,
            layer_name,
            worker_info,
        };

        let resp = client.request(Message::Hello).await?;
        client.worker_info = if let Message::WorkerInfo(info) = resp {
            info
        } else {
            return Err(anyhow!("unexpected worker info message: {:?}", &resp));
        };

        Ok(client)
    }

    async fn request(&mut self, req: Message) -> Result<Message> {
        req.to_writer(&mut self.stream).await?;
        super::Message::from_reader(&mut self.stream).await
    }

    async fn forward_request(&mut self, req: Message) -> Result<Tensor> {
        let resp = self.request(req).await?;
        match resp {
            Message::Tensor(raw) => Ok(raw.to_tensor(&self.device)?),
            _ => Err(anyhow!("unexpected response {:?}", &resp)),
        }
    }
}

impl std::fmt::Display for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}@{} [{}]",
            &self.layer_name, &self.address, &self.worker_info.device
        )
    }
}

#[async_trait]
impl super::Forwarder for Client {
    async fn forward(
        &mut self,
        x: &Tensor,
        index_pos: usize,
        block_idx: usize,
        _: &mut Cache,
    ) -> Result<Tensor> {
        self.forward_request(super::Message::transformer_op(
            &self.layer_name,
            x,
            index_pos,
            block_idx,
        ))
        .await
    }

    async fn forward_batch(
        &mut self,
        x: &Tensor,
        batch: Vec<(String, usize, usize)>,
        _: &mut Cache,
    ) -> Result<Tensor> {
        self.forward_request(super::Message::from_batch(x, batch))
            .await
    }

    fn ident(&self) -> &str {
        &self.address
    }

    fn layer_name(&self) -> &str {
        &self.layer_name
    }
}
