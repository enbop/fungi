use server_protobuf::server::{test_service_server::{TestService, TestServiceServer} , HelloReply, HelloRequest};
use tonic::{transport::Server, Request, Response, Status};

#[derive(Default)]
pub struct Tester {}

#[tonic::async_trait]
impl TestService for Tester {
    async fn greet(&self, request: Request<HelloRequest>) -> Result<Response<HelloReply>, Status> {
        println!("Got a request from {:?}", request.remote_addr());

        let reply = HelloReply {
            message: format!("Hello {}!", request.into_inner().name),
        };
        Ok(Response::new(reply))
    }
}

pub async fn start_grpc_service() {
    let addr = "[::1]:4242".parse().unwrap();
    let tester = Tester::default();

    Server::builder()
        .add_service(TestServiceServer::new(tester))
        .serve(addr)
        .await.unwrap();
}

#[cfg(test)]
mod test{
    #[tokio::test]
    async fn test_grpc_service() {
        super::start_grpc_service().await;
    }
}