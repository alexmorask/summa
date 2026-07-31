output "fqdn" {
  description = "Fully-qualified domain name of the Postgres server."
  value       = azurerm_postgresql_flexible_server.main.fqdn
}

output "server_name" {
  description = "Postgres Flexible Server name."
  value       = azurerm_postgresql_flexible_server.main.name
}

output "database_name" {
  description = "Name of the summa database on the server."
  value       = azurerm_postgresql_flexible_server_database.summa.name
}
