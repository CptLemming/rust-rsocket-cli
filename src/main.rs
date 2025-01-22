use std::collections::LinkedList;

use anyhow::anyhow;
use bytes::BytesMut;
use clap::{Args, Parser, Subcommand};
use prost::Message;
use prost_reflect::prost_types::FileDescriptorProto;
use prost_reflect::DescriptorPool;
use prost_reflect::DynamicMessage;
use prost_reflect::MessageDescriptor;
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
    command: Option<Commands>,
    endpoint: Option<String>,
    #[arg(short, long)]
    data: Option<String>,
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

#[derive(Args, Debug)]
struct StreamArgs {
    name: String,
}

#[derive(Args, Debug)]
struct SubscribeEventsArgs {}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    println!("Address : {:?}", cli.address);
    println!("Endpoint : {:?}", cli.endpoint);
    println!("Data : {:?}", cli.data);

    let client = RSocketFactory::connect()
        .transport(WebsocketClientTransport::from(cli.address.as_ref()))
        .setup(Payload::from("READY!"))
        .mime_type("text/plain", "text/plain")
        .start()
        .await?;

    match &cli.command {
        Some(Commands::List(args)) => {
            println!("List services: {:?}", args.name);
            list_services(&client).await?;
        }
        Some(Commands::Describe(args)) => {
            println!("Describe service: {:?}", args.name);
            describe_service(&client, &args.name).await?;
        }
        None => match &cli.endpoint {
            Some(endpoint) => {
                call_endpoint(&client, &endpoint, cli.data).await?;
            }
            _ => {}
        },
    }

    // client.close();

    Ok(())
}

async fn list_services(client: &Client) -> Result<()> {
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

async fn describe_service(client: &Client, name: &str) -> Result<DescriptorPool> {
    let req = reflection::ServerReflectionRequest {
        host: "ignore".to_owned(),
        message_request: Some(
            reflection::server_reflection_request::MessageRequest::FileByFilename(name.to_string()),
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
                        let mut pool = DescriptorPool::default();

                        let mut descriptors = vec![];

                        for file in fd.file_descriptor_proto {
                            // let content = String::from_utf8_lossy(&file);

                            // println!("File : {content:?}");

                            let fd = FileDescriptorProto::decode(file.as_ref());

                            // println!("DF : {fd:#?}");

                            if let Ok(fd) = fd {
                                descriptors.push(fd);
                            }
                        }
                        let res = pool.add_file_descriptor_protos(descriptors);
                        println!("RES : {res:?}");

                        return Ok(pool);
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

    Err(anyhow!("NoDescriptor"))
}

async fn call_endpoint(client: &Client, api: &str, data: Option<String>) -> Result<()> {
    // let api = "api.protobuf.routing.RoutingStrips.GetRoutingStrips";
    let (service, method) = api.split_once('/').unwrap();
    // let method = parts[parts.len() - 1];
    // let service = parts[..(parts.len() - 1)].join(".");

    println!("API : {api}");
    println!("Service : {service}");
    println!("Method : {method}");

    let pool = describe_service(client, service).await?;

    let service_descriptor = pool.get_service_by_name(&service).unwrap();
    // println!("DESC : {service_descriptor:?}");

    let package = service_descriptor.package_name();

    let mut input = None;
    let mut output = None;

    let mut is_stream_request = false;
    let mut is_stream_response = false;

    for method_descriptor in service_descriptor.methods() {
        if method_descriptor.name() == method {
            input = Some(method_descriptor.input().full_name().to_string());
            output = Some(method_descriptor.output().full_name().to_string());
            // println!("Method : {method:?}");
            // println!("Method:Input : {:?}", method.input());
            // println!("Method:Output : {:?}", method.output());

            is_stream_request = method_descriptor.is_client_streaming();
            is_stream_response = method_descriptor.is_server_streaming();
        }
    }

    println!("Package : {package:?}");
    println!("Input : {input:?}");
    println!("Output : {output:?}");

    let input = input.unwrap(); //format!("{}.{}", package, input.unwrap());
    let output = output.unwrap(); // format!("{}.{}", package, output.unwrap());

    let input_descriptor = pool.get_message_by_name(&input).unwrap();
    let output_descriptor = pool.get_message_by_name(&output).unwrap();

    let json = match data {
        Some(json) => json,
        // Default is empty payload
        None => r#"{}"#.to_string(),
    };

    let mut deserializer = serde_json::Deserializer::from_str(&json);
    let dynamic_message = DynamicMessage::deserialize(input_descriptor, &mut deserializer)?;
    deserializer.end()?;

    let payload = create_payload(
        dynamic_message,
        create_route(&format!("{service}.{method}"))?,
    )?;

    match (is_stream_request, is_stream_response) {
        (false, true) => {
            request_stream_endpoint(&client, payload, output_descriptor).await?;
        }
        (false, false) => {
            request_response_endpoint(&client, payload, output_descriptor).await?;
        }
        _ => {
            return Err(anyhow!("UnsupportedStream"));
        }
    }

    Ok(())
}

async fn request_stream_endpoint(
    client: &Client,
    payload: Payload,
    output_descriptor: MessageDescriptor,
) -> Result<()> {
    println!("Start request_stream");
    let mut results = client.request_stream(payload);

    loop {
        match results.next().await {
            Some(Ok(res)) => {
                // println!("Got : {:?}", res);

                let buf = res.data().unwrap().clone();
                // let dynamic_message = DynamicMessage::decode(message_descriptor.clone(), buf);
                let dynamic_message = DynamicMessage::decode(output_descriptor.clone(), buf);

                // println!("Response : {dynamic_message:#?}");
                if let Ok(message) = dynamic_message {
                    println!("JSON : {:?}", serde_json::json!(message));
                }
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

async fn request_response_endpoint(
    client: &Client,
    payload: Payload,
    output_descriptor: MessageDescriptor,
) -> Result<()> {
    println!("Start request_response");
    let results = client.request_response(payload).await?;

    if let Some(res) = results {
        let buf = res.data().unwrap().clone();
        // let dynamic_message = DynamicMessage::decode(message_descriptor.clone(), buf);
        let dynamic_message = DynamicMessage::decode(output_descriptor.clone(), buf);

        // println!("Response : {dynamic_message:#?}");
        if let Ok(message) = dynamic_message {
            println!("JSON : {:?}", serde_json::json!(message));
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
