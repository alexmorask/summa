data "terraform_remote_state" "foundation" {
  backend = "azurerm"
  config = {
    resource_group_name  = "summa-rg"
    storage_account_name = "summatfstate107ddf3a"
    container_name       = "tfstate"
    key                  = "summa.tfstate"
    use_azuread_auth     = true
  }
}

data "terraform_remote_state" "data" {
  backend = "azurerm"
  config = {
    resource_group_name  = "summa-rg"
    storage_account_name = "summatfstate107ddf3a"
    container_name       = "tfstate"
    key                  = "data.tfstate"
    use_azuread_auth     = true
  }
}

module "container_apps" {
  source = "../modules/container-apps"

  resource_group_name     = data.terraform_remote_state.foundation.outputs.resource_group_name
  location                = data.terraform_remote_state.foundation.outputs.location
  registry_id             = data.terraform_remote_state.foundation.outputs.registry_id
  registry_login_server   = data.terraform_remote_state.foundation.outputs.acr_login_server
  connection_string       = data.terraform_remote_state.data.outputs.connection_string
  postgres_host           = data.terraform_remote_state.data.outputs.fqdn
  postgres_admin_login    = "summaadmin"
  postgres_database       = data.terraform_remote_state.data.outputs.database_name
  postgres_admin_password = data.terraform_remote_state.foundation.outputs.postgres_admin_password
  image_tag               = var.image_tag
  auth_authority          = "https://login.microsoftonline.com/${data.terraform_remote_state.foundation.outputs.tenant_id}/v2.0"
  auth_audience           = data.terraform_remote_state.foundation.outputs.api_auth_client_id
  tags                    = { project = "summa" }
}
