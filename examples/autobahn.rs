use std::str::{self, FromStr};

use clap::Parser;
use http::uri::Uri;
use monoio::io::{AsyncReadRent, AsyncWriteRent, Split};
use monoio_ws::{Client, CloseCode, Config, Opcode};

// Agent name reported to the test server.
const AGENT: &str = "monoio-ws";

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "ws://localhost:9001")]
    uri: Uri,

    /// Optional test case number to run. If unset, runs all test cases.
    #[arg(short, long)]
    case: Option<usize>,
}

#[monoio::main]
async fn main() -> anyhow::Result<()> {
    let Args { uri, case } = Args::parse();

    if let Some(case) = case {
        // Run a specific test case.
        run_test_case(&uri, case).await?;
        update_reports(&uri).await?;
    } else {
        // Run all test cases.
        let case_count = get_case_count(&uri).await?;

        if case_count > 0 {
            println!("Running {case_count} test cases.");

            for case in 1..=case_count {
                run_test_case(&uri, case).await?;
            }

            update_reports(&uri).await?;
        }
    }

    println!("All tests completed!");
    Ok(())
}

async fn get_case_count(uri: &Uri) -> anyhow::Result<usize> {
    let uri = Uri::from_str(&format!("{uri}getCaseCount"))?;

    println!("Connecting via {uri}.");
    let mut client = Client::connect_plain(&uri, &Config::default()).await?;
    println!("Connected.");

    let frame = client.read_frame().await?;
    println!("Received {frame:?}");
    assert!(matches!(frame.opcode, Opcode::Text));
    let text = str::from_utf8(frame.data)?;
    let case_count = text.parse::<usize>()?;

    client
        .send_close(&u16::from(CloseCode::Normal).to_be_bytes())
        .await?;

    println!("Read test count: {case_count}.");
    Ok(case_count)
}

async fn run_test_case(uri: &Uri, case: usize) -> anyhow::Result<()> {
    let uri = Uri::from_str(&format!("{uri}runCase?case={case}&agent={AGENT}"))?;

    println!("Connecting via {uri}.");
    let mut client = Client::connect_plain(&uri, &Config::default()).await?;
    println!("Connected.");

    let mut buffer = Vec::with_capacity(128 * 1024);

    loop {
        let (res, buf) = process_message(&mut client, buffer).await;
        match res {
            Ok(()) => {
                buffer = buf;
            }
            Err(monoio_ws::Error::ProtocolViolation(_) | monoio_ws::Error::Closed { .. }) => {
                break;
            }
            Err(e) => {
                eprintln!("Test case {case} failed: {e:?}");
                break;
            }
        }
    }

    Ok(())
}

async fn process_message(
    client: &mut Client<impl AsyncReadRent + AsyncWriteRent + Split>,
    buffer: Vec<u8>,
) -> monoio_ws::BufResult<()> {
    let (res, buffer) = client.next_msg(buffer).await;
    match res {
        Ok(msg) => {
            if let Err(e) = if msg.is_text() {
                client.send_text(&buffer).await
            } else {
                client.send_binary(&buffer).await
            } {
                (Err(e.into()), buffer)
            } else {
                (Ok(()), buffer)
            }
        }
        Err(e) => (Err(e), buffer),
    }
}

async fn update_reports(uri: &Uri) -> anyhow::Result<()> {
    let uri = Uri::from_str(&format!("{uri}updateReports?agent={AGENT}"))?;

    println!("Connecting via {uri}.");
    let mut client = Client::connect_plain(&uri, &Config::default()).await?;
    println!("Connected.");

    // Wait for the server to close the connection.
    let frame = client.read_frame().await?;
    assert!(matches!(frame.opcode, Opcode::Close));

    println!("Updated reports.");
    Ok(())
}
