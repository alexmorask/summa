locals {
  tags = {
    project = "summa"
  }

  admin_object_id = "1500807d-dd8f-4cbb-a337-0a06dd01b52c"
  tenant_id       = "785523cd-52b4-4167-a4e6-9f116d688c0f"
}

resource "random_id" "suffix" {
  byte_length = 4
}

resource "azurerm_resource_group" "main" {
  name     = "summa-rg"
  location = var.location

  tags = local.tags
}

resource "azurerm_storage_account" "tfstate" {
  name                = "summatfstate${random_id.suffix.hex}"
  resource_group_name = azurerm_resource_group.main.name
  location            = azurerm_resource_group.main.location

  account_tier             = "Standard"
  account_replication_type = "LRS"

  allow_nested_items_to_be_public = false
  shared_access_key_enabled       = false

  tags = local.tags
}

resource "azurerm_storage_container" "tfstate" {
  name                  = "tfstate"
  storage_account_id    = azurerm_storage_account.tfstate.id
  container_access_type = "private"
}

resource "azurerm_role_assignment" "current_user_tfstate" {
  scope                = azurerm_storage_account.tfstate.id
  role_definition_name = "Storage Blob Data Contributor"
  principal_id         = local.admin_object_id
}

resource "random_password" "postgres_admin" {
  length           = 32
  override_special = "!#$%&*()-_+[]{}<>?"
  min_upper        = 1
  min_lower        = 1
  min_numeric      = 1
  min_special      = 1
}

resource "azurerm_key_vault" "main" {
  name                = "summa-kv-${random_id.suffix.hex}"
  resource_group_name = azurerm_resource_group.main.name
  location            = azurerm_resource_group.main.location
  tenant_id           = local.tenant_id
  sku_name            = "standard"

  rbac_authorization_enabled = true
  purge_protection_enabled   = true

  tags = local.tags
}

resource "azurerm_role_assignment" "current_user_kv_secrets_officer" {
  scope                = azurerm_key_vault.main.id
  role_definition_name = "Key Vault Secrets Officer"
  principal_id         = local.admin_object_id
}

resource "azurerm_key_vault_secret" "postgres_admin_password" {
  name         = "postgres-admin-password"
  value        = random_password.postgres_admin.result
  key_vault_id = azurerm_key_vault.main.id

  depends_on = [azurerm_role_assignment.current_user_kv_secrets_officer]

  tags = local.tags
}

module "registry" {
  source = "./modules/registry"

  name                = "summacr${random_id.suffix.hex}"
  resource_group_name = azurerm_resource_group.main.name
  location            = azurerm_resource_group.main.location
  tags                = local.tags
}

module "api_auth" {
  source = "./modules/api-auth"

  admin_object_id = local.admin_object_id
}

module "github_oidc" {
  source = "./modules/github-oidc"

  github_repository_id       = "alexmorask@6801050/summa@1315397592"
  resource_group_id          = azurerm_resource_group.main.id
  tfstate_storage_account_id = azurerm_storage_account.tfstate.id
  admin_object_id            = local.admin_object_id
  registry_id                = module.registry.id
  key_vault_id               = azurerm_key_vault.main.id
}

resource "azurerm_consumption_budget_resource_group" "main" {
  name              = "summa-budget"
  resource_group_id = azurerm_resource_group.main.id

  amount     = var.budget_amount
  time_grain = "Monthly"

  time_period {
    start_date = "2026-08-01T00:00:00Z"
  }

  notification {
    enabled        = true
    threshold      = 80
    operator       = "GreaterThan"
    contact_emails = [var.budget_alert_email]
  }

  notification {
    enabled        = true
    threshold      = 100
    operator       = "GreaterThan"
    contact_emails = [var.budget_alert_email]
  }
}
