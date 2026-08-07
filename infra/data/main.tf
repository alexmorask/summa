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

module "database" {
  source = "../modules/database"

  name                   = "summa-postgres-107ddf3a"
  resource_group_name    = data.terraform_remote_state.foundation.outputs.resource_group_name
  location               = data.terraform_remote_state.foundation.outputs.location
  administrator_login    = "summaadmin"
  administrator_password = data.terraform_remote_state.foundation.outputs.postgres_admin_password
  admin_ip_address       = var.admin_ip_address
  tags                   = { project = "summa" }
}
