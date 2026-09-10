namespace Summa.Policy.Store

open System.Text.Json
open System.Text.Json.Serialization
open Summa.Policy.Domain

module Serialization =

    let options =
        JsonFSharpOptions.Default()
            .WithUnionUnwrapFieldlessTags()
            .ToJsonSerializerOptions()

    let serialize (pricing: PricingFormula) : string =
        JsonSerializer.Serialize(pricing, options)

    let deserialize (json: string) : PricingFormula =
        JsonSerializer.Deserialize<PricingFormula>(json, options)
