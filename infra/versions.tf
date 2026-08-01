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
    azuread = {
      source  = "hashicorp/azuread"
      version = "~> 3.9"
    }
  }

  backend "azurerm" {
    resource_group_name  = "summa-rg"
    storage_account_name = "summatfstate107ddf3a"
    container_name       = "tfstate"
    key                  = "summa.tfstate"
    use_azuread_auth     = true
  }
}

provider "azurerm" {
  features {}

  storage_use_azuread = true
}

provider "azuread" {}
