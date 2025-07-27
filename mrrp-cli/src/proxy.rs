use tokio::net::TcpListener;

use crate::Error;

pub async fn serve(input_address: &str, output_address: &str) -> Result<(), Error> {
    let _input_listener = TcpListener::bind(input_address).await?;

    //let output = RtlTcpServer::new(, tcp_listener)

    //let output = RtlTcpServer::new(ProxyBackend)
    let _output_listener = TcpListener::bind(output_address).await?;

    todo!();
}
