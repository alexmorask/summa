FROM mcr.microsoft.com/dotnet/sdk:10.0 AS build
WORKDIR /src

COPY src/Summa.Ledger.Domain/Summa.Ledger.Domain.fsproj src/Summa.Ledger.Domain/
COPY src/Summa.Ledger.Store/Summa.Ledger.Store.fsproj src/Summa.Ledger.Store/
COPY src/Summa.Recognition.Domain/Summa.Recognition.Domain.fsproj src/Summa.Recognition.Domain/
COPY src/Summa.Recognition.Store/Summa.Recognition.Store.fsproj src/Summa.Recognition.Store/
COPY src/Summa.Recognition.Job/Summa.Recognition.Job.fsproj src/Summa.Recognition.Job/
RUN dotnet restore src/Summa.Recognition.Job/Summa.Recognition.Job.fsproj

COPY src/Summa.Ledger.Domain/ src/Summa.Ledger.Domain/
COPY src/Summa.Ledger.Store/ src/Summa.Ledger.Store/
COPY src/Summa.Recognition.Domain/ src/Summa.Recognition.Domain/
COPY src/Summa.Recognition.Store/ src/Summa.Recognition.Store/
COPY src/Summa.Recognition.Job/ src/Summa.Recognition.Job/
RUN dotnet publish src/Summa.Recognition.Job/Summa.Recognition.Job.fsproj -c Release -o /app --no-restore

FROM mcr.microsoft.com/dotnet/runtime:10.0 AS final
WORKDIR /app
COPY --from=build /app .
USER $APP_UID
ENTRYPOINT ["dotnet", "Summa.Recognition.Job.dll"]
