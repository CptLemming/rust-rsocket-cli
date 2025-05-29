use std::collections::LinkedList;

use anyhow::anyhow;
use bytes::BufMut;
use bytes::BytesMut;
use clap::ArgAction;
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
  #[arg(short, long, action=ArgAction::SetTrue)]
  pretty: Option<bool>,
  #[arg(short, long)]
  token: Option<String>,
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

  let connection_payload = cli
    .token
    .and_then(|token| create_setup(token.as_str()).ok())
    .and_then(|metadata| create_setup_payload(metadata).ok())
    .unwrap_or_else(|| Payload::from(""));

  let client = RSocketFactory::connect()
    .transport(WebsocketClientTransport::from(cli.address.as_ref()))
    .setup(connection_payload)
    .mime_type("text/plain", "text/plain")
    .start()
    .await?;

  match &cli.command {
    Some(Commands::List(args)) => {
      list_services(&client, &args.name).await?;
    }
    Some(Commands::Describe(args)) => {
      describe_service(&client, &args.name, true).await?;
    }
    None => match &cli.endpoint {
      Some(endpoint) => {
        call_endpoint(&client, &endpoint, cli.data, cli.pretty.unwrap_or_default()).await?;
      }
      _ => {}
    },
  }

  Ok(())
}

async fn list_services(client: &Client, _service: &Option<String>) -> Result<()> {
  let req = reflection::ServerReflectionRequest {
    host: "ignore".to_owned(),
    message_request: Some(reflection::server_reflection_request::MessageRequest::ListServices(
      "ignore".to_owned(),
    )),
  };

  let payload = create_payload(
    req,
    create_route("grpc.reflection.v1.ServerReflection.ServerReflectionInfo")?,
  )?;

  let stream = async_stream::stream! {
      yield Ok(payload);
  };

  let mut results = client.request_channel(Box::pin(stream));

  loop {
    match results.next().await {
      Some(Ok(res)) => {
        let buf = res.data().unwrap().clone();
        let decoded = reflection::ServerReflectionResponse::decode(buf)?;

        match decoded.message_response {
          Some(reflection::server_reflection_response::MessageResponse::ListServicesResponse(ls)) => {
            for service in ls.service {
              println!("{}", service.name);
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

async fn describe_service(client: &Client, name: &str, with_output: bool) -> Result<DescriptorPool> {
  let (service_name, method_name) = name.split_once("/").unwrap_or((name, ""));

  let req = reflection::ServerReflectionRequest {
    host: "ignore".to_owned(),
    message_request: Some(reflection::server_reflection_request::MessageRequest::FileByFilename(
      service_name.to_string(),
    )),
  };

  let payload = create_payload(
    req,
    create_route("grpc.reflection.v1.ServerReflection.ServerReflectionInfo")?,
  )?;

  let stream = async_stream::stream! {
      yield Ok(payload);
  };

  let mut results = client.request_channel(Box::pin(stream));

  loop {
    match results.next().await {
      Some(Ok(res)) => {
        let buf = res.data().unwrap().clone();
        let decoded = reflection::ServerReflectionResponse::decode(buf)?;

        match decoded.message_response {
          Some(reflection::server_reflection_response::MessageResponse::FileDescriptorResponse(fd)) => {
            let mut pool = DescriptorPool::default();

            let mut descriptors = vec![];

            for file in fd.file_descriptor_proto {
              let fd = FileDescriptorProto::decode(file.as_ref());

              if let Ok(fd) = fd {
                descriptors.push(fd);
              }
            }
            if let Err(err) = pool.add_file_descriptor_protos(descriptors) {
              eprintln!("Error : {err}");
              break;
            }

            if with_output {
              for service in pool.services() {
                if method_name.is_empty() {
                  println!("{} is a service:\n", service.full_name());
                  println!("service {} {{", service.name());
                }

                for method in service.methods() {
                  if method_name.is_empty() {
                    println!(
                      "  rpc {}( {}{} ) returns ( {}{} );",
                      method.name(),
                      if method.is_client_streaming() { "stream " } else { "" },
                      method.input().full_name(),
                      if method.is_server_streaming() { "stream " } else { "" },
                      method.output().full_name()
                    );
                  }

                  if !method_name.is_empty() && method.name() == method_name {
                    println!("{} is a method:\n", method.full_name());
                    println!(
                      "  rpc {}( {}{} ) returns ( {}{} );",
                      method.name(),
                      if method.is_client_streaming() { "stream " } else { "" },
                      method.input().full_name(),
                      if method.is_server_streaming() { "stream " } else { "" },
                      method.output().full_name()
                    );
                  }
                }

                if method_name.is_empty() {
                  println!("}}");
                }
              }
            }

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
        eprintln!("Ended stream");
        break;
      }
    }
  }

  Err(anyhow!("NoDescriptor"))
}

async fn call_endpoint(client: &Client, api: &str, data: Option<String>, pretty: bool) -> Result<()> {
  let (service, method) = api.split_once('/').unwrap();

  let pool = describe_service(client, service, false).await?;

  let service_descriptor = pool.get_service_by_name(&service).unwrap();

  let mut input = None;
  let mut output = None;

  let mut is_stream_request = false;
  let mut is_stream_response = false;

  for method_descriptor in service_descriptor.methods() {
    if method_descriptor.name() == method {
      input = Some(method_descriptor.input().full_name().to_string());
      output = Some(method_descriptor.output().full_name().to_string());

      is_stream_request = method_descriptor.is_client_streaming();
      is_stream_response = method_descriptor.is_server_streaming();
    }
  }

  let input = input.unwrap();
  let output = output.unwrap();

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

  let payload = create_payload(dynamic_message, create_route(&format!("{service}.{method}"))?)?;

  match (is_stream_request, is_stream_response) {
    (false, true) => {
      request_stream_endpoint(&client, payload, output_descriptor, pretty).await?;
    }
    (false, false) => {
      request_response_endpoint(&client, payload, output_descriptor, pretty).await?;
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
  pretty: bool,
) -> Result<()> {
  let mut results = client.request_stream(payload);

  loop {
    match results.next().await {
      Some(Ok(res)) => {
        let buf = res.data().unwrap().clone();
        let dynamic_message = DynamicMessage::decode(output_descriptor.clone(), buf);

        if let Ok(message) = dynamic_message {
          let content = serde_json::json!(message);
          if pretty {
            if let Ok(content) = serde_json::to_string_pretty(&content) {
              println!("{}", content);
            }
          } else {
            println!("{}", content);
          }
        }
      }
      Some(Err(err)) => {
        eprintln!("ERR : {err:?}");
        break;
      }
      None => {
        eprintln!("Ended stream");
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
  pretty: bool,
) -> Result<()> {
  let results = client.request_response(payload).await?;

  if let Some(res) = results {
    if let Some(buf) = res.data() {
      let buf = buf.clone();
      let dynamic_message = DynamicMessage::decode(output_descriptor.clone(), buf);

      if let Ok(message) = dynamic_message {
        let content = serde_json::json!(message);
        if pretty {
          if let Ok(content) = serde_json::to_string_pretty(&content) {
            println!("{}", content);
          }
        } else {
          println!("{}", content);
        }
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

fn create_setup(token: &str) -> Result<CompositeMetadata> {
  let mut buf = BytesMut::new();

  // Does not exist in Rust lib. JavaScript implementation:
  // function encodeBearerAuthMetadata(token) {
  //     var tokenBuffer = Buffer.from(token);
  //     var buffer = Buffer.allocUnsafe(authTypeIdBytesLength);
  //     // eslint-disable-next-line no-bitwise
  //     buffer.writeUInt8(WellKnownAuthType_1.WellKnownAuthType.BEARER.identifier | streamMetadataKnownMask);
  //     return Buffer.concat([buffer, tokenBuffer]);
  // }
  let stream_metadata_known_mask = 0x80;
  let bearer_ident = 0x01;
  buf.put_u8(bearer_ident | stream_metadata_known_mask);
  buf.put_slice(token.as_bytes());

  let mut metadata: LinkedList<FnMetadata> = LinkedList::new();
  metadata.push_back(Box::new(move || {
    Ok((MimeType::MESSAGE_X_RSOCKET_AUTHENTICATION_V0, buf.to_vec()))
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

fn create_setup_payload(metadata: CompositeMetadata) -> Result<Payload> {
  let request_payload = Payload::builder().set_metadata(metadata.bytes()).build();

  Ok(request_payload)
}
