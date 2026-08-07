variable "admin_ip_address" {
  description = "Your current public IP, allowlisted on the Postgres firewall for migrations/admin access. Look it up yourself (e.g. `curl https://api.ipify.org`) and pass it explicitly via -var or TF_VAR_admin_ip_address — deliberately not fetched automatically by Terraform, since piping an unauthenticated third-party service's response straight into a database firewall rule with no human review is a real supply-chain risk, not just a convenience trade-off."
  type        = string
}
