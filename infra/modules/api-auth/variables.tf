variable "admin_object_id" {
  description = "Fixed Entra object ID of the human administrator who owns these App Registrations and Service Principals — not derived from whoever is currently running Terraform, matching the github-oidc module's convention."
  type        = string
}
