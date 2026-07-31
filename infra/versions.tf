terraform {
  required_version = ">= 1.15"

  required_providers {
    azurerm = {
      source  = "hashicorp/azurerm"
      version = "~> 4.2"
    }
    random = {
      source  = "hashicorp/random"
      version = "~> 3.6"
    }
  }

  # Commented out for the bootstrap apply: this backend can't be activated
  # until the state storage account it points at (created below, in
  # main.tf) exists, and Terraform 1.15 refuses to plan/apply at all with
  # an uninitialized non-default backend block present, even with
  # `init -backend=false`. First apply runs with plain local state; once it
  # succeeds, uncomment this, run `terraform init -migrate-state` with the
  # real storage account details (via -backend-config flags or a
  # backend.hcl file, not committed), and local state moves into it.
  # backend "azurerm" {}
}

provider "azurerm" {
  features {}
}
