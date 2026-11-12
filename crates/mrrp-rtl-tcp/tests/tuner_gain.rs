use mrrp_rtl_tcp::client::{
    Client,
    ConnectOptions,
    TunerGain,
};
use tokio::{
    io::{
        AsyncReadExt,
        AsyncWriteExt,
    },
    net::TcpListener,
};

#[tokio::test]
async fn sends_tuner_gain_commands_in_protocol_order() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = async {
        let (mut socket, _) = listener.accept().await.unwrap();
        socket.write_all(b"RTL0\0\0\0\x05\0\0\0\x08").await.unwrap();
        let mut commands = Vec::new();
        socket.read_to_end(&mut commands).await.unwrap();
        commands
    };
    let client = async {
        let mut client = Client::connect(address, ConnectOptions::default())
            .await
            .unwrap();
        for gain in [
            TunerGain::Auto,
            TunerGain::Index(3),
            TunerGain::TenthsDb(-15),
        ] {
            client.command_sender.set_tuner_gain(gain).await.unwrap();
        }
    };

    let (commands, ()) = tokio::join!(server, client);
    assert_eq!(
        commands,
        [
            [0x03, 0, 0, 0, 0],
            [0x03, 0, 0, 0, 1],
            [0x0d, 0, 0, 0, 3],
            [0x03, 0, 0, 0, 1],
            [0x04, 0xff, 0xff, 0xff, 0xf1],
        ]
        .concat()
    );
}
