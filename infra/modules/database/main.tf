resource "azurerm_postgresql_flexible_server" "main" {
  name                = var.name
  resource_group_name = var.resource_group_name
  location            = var.location

  version = "17"

  administrator_login    = var.administrator_login
  administrator_password = var.administrator_password

  storage_mb = 32768
  sku_name   = "B_Standard_B1ms"

  public_network_access_enabled = true

  tags = var.tags

  lifecycle {
    ignore_changes = [zone]
  }
}

resource "azurerm_postgresql_flexible_server_database" "summa" {
  name      = "summa"
  server_id = azurerm_postgresql_flexible_server.main.id
}

resource "azurerm_postgresql_flexible_server_firewall_rule" "allow_admin_ip" {
  name             = "AllowAdminIp"
  server_id        = azurerm_postgresql_flexible_server.main.id
  start_ip_address = var.admin_ip_address
  end_ip_address   = var.admin_ip_address
}

resource "azurerm_postgresql_flexible_server_firewall_rule" "allow_azure_services" {
  name             = "AllowAzureServices"
  server_id        = azurerm_postgresql_flexible_server.main.id
  start_ip_address = "0.0.0.0"
  end_ip_address   = "0.0.0.0"
}
