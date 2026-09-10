namespace Summa.Policy.Api

open System
open Summa.Policy.Domain

type PublishPolicyRequest =
    { IdempotencyKey : string
      Pricing        : PricingFormula }

type PublishPolicyResponse = { Id : Guid }

type ErrorResponse = { Error : string }
