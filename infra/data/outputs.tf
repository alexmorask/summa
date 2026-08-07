output "fqdn" {
  description = "Fully-qualified domain name of the Postgres server."
  value       = module.database.fqdn
}

output "server_name" {
  description = "Postgres Flexible Server name."
  value       = module.database.server_name
}

output "database_name" {
  description = "Name of the summa database on the server."
  value       = module.database.database_name
}

output "connection_string" {
  description = "Postgres connection string for consumers of this layer's state."
  value       = "Host=${module.database.fqdn};Port=5432;Username=summaadmin;Password=${data.terraform_remote_state.foundation.outputs.postgres_admin_password};Database=${module.database.database_name};Ssl Mode=VerifyFull"
  sensitive   = true
}
