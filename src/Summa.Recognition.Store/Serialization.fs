namespace Summa.Recognition.Store

open System.Text.Json
open System.Text.Json.Serialization
open Summa.Recognition.Domain

module Serialization =

    let options =
        JsonFSharpOptions.Default()
            .WithUnionUnwrapFieldlessTags()
            .ToJsonSerializerOptions()

    let serialize (obligation: PerformanceObligation) : string =
        JsonSerializer.Serialize(obligation, options)

    let deserialize (json: string) : PerformanceObligation =
        JsonSerializer.Deserialize<PerformanceObligation>(json, options)
