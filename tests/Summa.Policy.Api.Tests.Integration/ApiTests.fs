module ApiTests

open System
open System.Net
open System.Net.Http
open System.Net.Http.Headers
open System.Net.Http.Json
open System.Text
open System.Text.Json
open System.Threading.Tasks
open Microsoft.AspNetCore.Hosting
open Microsoft.AspNetCore.Mvc.Testing
open Xunit
open Summa.Policy.Api
open Summa.Policy.Api.Program
open Summa.Policy.Domain
open Summa.Policy.Store

let private requiredEnv (name: string) : string =
    match Environment.GetEnvironmentVariable name with
    | null -> failwith $"{name} must be set to run integration tests (see infra/modules/policy-auth's test_client)."
    | value -> value

let private getTestToken () : string =
    let tenantId = "785523cd-52b4-4167-a4e6-9f116d688c0f"
    let apiAudience = "00000000-0000-0000-0000-000000000000"
    let clientId = requiredEnv "TEST_CLIENT_ID"
    let clientSecret = requiredEnv "TEST_CLIENT_SECRET"

    use tokenClient = new HttpClient()
    use content =
        new FormUrlEncodedContent(
            dict [ "grant_type", "client_credentials"
                   "client_id", clientId
                   "client_secret", clientSecret
                   "scope", $"{apiAudience}/.default" ]
        )

    let response =
        tokenClient.PostAsync($"https://login.microsoftonline.com/{tenantId}/oauth2/v2.0/token", content)
        |> Async.AwaitTask
        |> Async.RunSynchronously

    let json =
        response.Content.ReadAsStringAsync()
        |> Async.AwaitTask
        |> Async.RunSynchronously

    use doc = JsonDocument.Parse(json)
    doc.RootElement.GetProperty("access_token").GetString()

let private factory =
    (new WebApplicationFactory<Program>())
        .WithWebHostBuilder(fun builder -> builder.UseEnvironment("Development") |> ignore)

let private client = factory.CreateClient()
client.DefaultRequestHeaders.Authorization <- AuthenticationHeaderValue("Bearer", getTestToken ())

let private request idempotencyKey pricing : PublishPolicyRequest =
    { IdempotencyKey = idempotencyKey
      Pricing = pricing }

let private samplePricing =
    Sum [ Flat 5000L
          PerUnit(UnitOfMeasure "gb", 100L) ]

[<Fact>]
let ``GET health returns 200 OK`` () : Task =
    task {
        let! response = client.GetAsync("/health")
        Assert.Equal(HttpStatusCode.OK, response.StatusCode)
    }

[<Fact>]
let ``POST policies with a valid pricing tree returns 200 with a policy id`` () : Task =
    task {
        let body = request (Guid.NewGuid().ToString()) samplePricing

        let! response = client.PostAsJsonAsync("/policies", body, Serialization.options)

        Assert.Equal(HttpStatusCode.OK, response.StatusCode)
        let! published = response.Content.ReadFromJsonAsync<PublishPolicyResponse>(Serialization.options)
        Assert.NotEqual(Guid.Empty, published.Id)
    }

[<Fact>]
let ``POST policies with the same idempotency key twice returns the same id`` () : Task =
    task {
        let body = request (Guid.NewGuid().ToString()) samplePricing

        let! first = client.PostAsJsonAsync("/policies", body, Serialization.options)
        let! firstPublished = first.Content.ReadFromJsonAsync<PublishPolicyResponse>(Serialization.options)
        let! second = client.PostAsJsonAsync("/policies", body, Serialization.options)
        let! secondPublished = second.Content.ReadFromJsonAsync<PublishPolicyResponse>(Serialization.options)

        Assert.Equal(HttpStatusCode.OK, second.StatusCode)
        Assert.Equal(firstPublished.Id, secondPublished.Id)
    }

[<Fact>]
let ``POST policies with an empty Sum returns 400 with a domain error message`` () : Task =
    task {
        let body = request (Guid.NewGuid().ToString()) (Sum [])

        let! response = client.PostAsJsonAsync("/policies", body, Serialization.options)

        Assert.Equal(HttpStatusCode.BadRequest, response.StatusCode)
        let! error = response.Content.ReadFromJsonAsync<ErrorResponse>(Serialization.options)
        Assert.Equal("a Sum must contain at least one component", error.Error)
    }

[<Fact>]
let ``POST policies with malformed JSON returns 400`` () : Task =
    task {
        use content = new StringContent("{not valid json", Encoding.UTF8, "application/json")
        let! response = client.PostAsync("/policies", content)

        Assert.Equal(HttpStatusCode.BadRequest, response.StatusCode)
        let! error = response.Content.ReadFromJsonAsync<ErrorResponse>(Serialization.options)
        Assert.Equal("malformed JSON body", error.Error)
    }

[<Fact>]
let ``GET policies round-trips a published policy`` () : Task =
    task {
        let body = request (Guid.NewGuid().ToString()) samplePricing
        let! posted = client.PostAsJsonAsync("/policies", body, Serialization.options)
        let! published = posted.Content.ReadFromJsonAsync<PublishPolicyResponse>(Serialization.options)

        let! response = client.GetAsync($"/policies/{published.Id}")

        Assert.Equal(HttpStatusCode.OK, response.StatusCode)
        let! policy = response.Content.ReadFromJsonAsync<Policy>(Serialization.options)
        Assert.Equal(published.Id, policy.Id)
        Assert.Equal(samplePricing, policy.Pricing)
    }

[<Fact>]
let ``GET policies for an unknown id returns 404`` () : Task =
    task {
        let! response = client.GetAsync($"/policies/{Guid.NewGuid()}")

        Assert.Equal(HttpStatusCode.NotFound, response.StatusCode)
        let! error = response.Content.ReadFromJsonAsync<ErrorResponse>(Serialization.options)
        Assert.Equal("no such policy", error.Error)
    }
