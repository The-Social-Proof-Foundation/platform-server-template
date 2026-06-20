use myso_rpc::Client;

pub fn create_client(grpc_url: &str) -> Result<Client, tonic::Status> {
    Client::new(grpc_url)
}

pub async fn check_startup_status(grpc_url: &str) -> Result<(), String> {
    let mut client = create_client(grpc_url).map_err(|e| e.to_string())?;
    let _ = client
        .ledger_client()
        .get_service_info(myso_rpc::proto::myso::rpc::v2::GetServiceInfoRequest::default())
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
