use std::collections::LinkedList;

use bytes::BytesMut;
use clap::{Args, Parser, Subcommand};
use prost::Message;
use rsocket_rust::extension::CompositeMetadata;
use rsocket_rust::extension::MimeType;
use rsocket_rust::extension::RoutingMetadata;
use rsocket_rust::prelude::*;
use rsocket_rust::utils::Writeable;
use rsocket_rust::Client;
use rsocket_rust::Result;
use rsocket_rust_transport_websocket::WebsocketClientTransport;

type FnMetadata = Box<dyn FnMut() -> Result<(MimeType, Vec<u8>)>>;

pub mod reflection {
    include!(concat!(env!("OUT_DIR"), "/grpc.reflection.v1.rs"));
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    address: String,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    List(ListArgs),
    Describe(DescribeArgs),
}

#[derive(Args, Debug)]
struct ListArgs {
    name: Option<String>,
}

#[derive(Args, Debug)]
struct DescribeArgs {
    name: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    println!("Address : {:?}", cli.address);

    let client = RSocketFactory::connect()
        .transport(WebsocketClientTransport::from(cli.address.as_ref()))
        .setup(Payload::from("READY!"))
        .mime_type("text/plain", "text/plain")
        .start()
        .await?;

    match &cli.command {
        Commands::List(args) => {
            println!("List services: {:?}", args.name);
            list_services(client).await?;
        }
        Commands::Describe(args) => {
            println!("Describe service: {:?}", args.name);
            describe_service(client, args.name.clone()).await?;
        }
    }

    // client.close();

    Ok(())
}

async fn list_services(client: Client) -> Result<()> {
    let req = reflection::ServerReflectionRequest {
        host: "ignore".to_owned(),
        message_request: Some(
            reflection::server_reflection_request::MessageRequest::ListServices(
                "ignore".to_owned(),
            ),
        ),
    };

    let payload = create_payload(
        req,
        create_route("grpc.reflection.v1.ServerReflection.ServerReflectionInfo")?,
    )?;

    let stream = async_stream::stream! {
        yield Ok(payload);
    };

    println!("Start channel");
    let mut results = client.request_channel(Box::pin(stream));

    loop {
        match results.next().await {
            Some(Ok(res)) => {
                println!("Got : {:?}", res);

                let buf = res.data().unwrap().clone();
                let decoded = reflection::ServerReflectionResponse::decode(buf)?;

                match decoded.message_response {
                    Some(reflection::server_reflection_response::MessageResponse::ListServicesResponse(ls)) => {
                        for service in ls.service {
                            println!("Service : {:?}", service.name);
                        }
                    }
                    _ => {}
                }

                // println!("Decoded : {decoded:#?}");
                break;
            }
            Some(Err(err)) => {
                eprintln!("ERR : {err:?}");
                break;
            }
            None => {
                println!("Ended stream");
                break;
            }
        }
    }

    Ok(())
}

async fn describe_service(client: Client, name: String) -> Result<()> {
    let req = reflection::ServerReflectionRequest {
        host: "ignore".to_owned(),
        message_request: Some(
            reflection::server_reflection_request::MessageRequest::FileByFilename(name),
        ),
    };

    let payload = create_payload(
        req,
        create_route("grpc.reflection.v1.ServerReflection.ServerReflectionInfo")?,
    )?;

    let stream = async_stream::stream! {
        yield Ok(payload);
    };

    println!("Start channel");
    let mut results = client.request_channel(Box::pin(stream));

    loop {
        match results.next().await {
            Some(Ok(res)) => {
                println!("Got : {:?}", res);

                let buf = res.data().unwrap().clone();
                let decoded = reflection::ServerReflectionResponse::decode(buf)?;

                // println!("Decoded : {decoded:?}");

                match decoded.message_response {
                    Some(reflection::server_reflection_response::MessageResponse::FileDescriptorResponse(fd)) => {
                        for file in fd.file_descriptor_proto {
                            let content = String::from_utf8_lossy(&file);

                            println!("File : {content:?}");
                        }
                    }
                    _ => {}
                }

                break;
            }
            Some(Err(err)) => {
                eprintln!("ERR : {err:?}");
                break;
            }
            None => {
                println!("Ended stream");
                break;
            }
        }
    }

    Ok(())
}

fn create_route(name: &str) -> Result<CompositeMetadata> {
    let routing = RoutingMetadata::builder().push_str(name).build();
    let mut buf = BytesMut::new();
    routing.write_to(&mut buf);

    let mut metadata: LinkedList<FnMetadata> = LinkedList::new();
    metadata.push_back(Box::new(move || {
        Ok((MimeType::MESSAGE_X_RSOCKET_ROUTING_V0, buf.to_vec()))
    }));

    let mut composite = CompositeMetadata::builder();

    for mut boxed in metadata.into_iter() {
        let (mime_type, raw) = boxed()?;
        composite = composite.push(mime_type, raw);
    }

    Ok(composite.build())
}

fn create_payload(data: impl Message, metadata: CompositeMetadata) -> Result<Payload> {
    let request_payload = Payload::builder()
        .set_data(data.encode_to_vec())
        .set_metadata(metadata.bytes())
        .build();

    Ok(request_payload)
}
