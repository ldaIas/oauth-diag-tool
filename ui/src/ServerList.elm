module ServerList exposing (Action(..), Client, Model, Msg(..), OAuthServer, init, serverConfigDecoder, update, view)

import Html exposing (..)
import Html.Attributes exposing (class, disabled)
import Html.Events exposing (onClick)
import Json.Decode as Decode
import Set exposing (Set)


-- MODEL


type alias Client =
    { id : String
    , clientId : String
    , clientSecret : String
    }


type alias OAuthServer =
    { id : String
    , name : String
    , issuerUrl : String
    , authorizationUrl : String
    , tokenEndpoint : String
    , port_ : Int
    , running : Bool
    , clients : List Client
    }


type alias Model =
    { servers : List OAuthServer
    , expandedClients : Set String
    }


init : Model
init =
    { servers = []
    , expandedClients = Set.empty
    }



-- UPDATE


type Msg
    = OpenCreateForm
    | DeleteServer String
    | AddClient String
    | DeleteClient String
    | ImportClient OAuthServer Client
    | StartServer String
    | StopServer String
    | ToggleClients String
    | GotServerConfigs (Result Decode.Error (List OAuthServer))


type Action
    = RequestCreateForm Int
    | RequestDeleteServer String
    | RequestAddClient String
    | RequestDeleteClient String
    | RequestImportClient { name : String, issuerUrl : String, authorizationUrl : String, tokenUrl : String, clientId : String, clientSecret : String }
    | RequestStartServer String
    | RequestStopServer String
    | NoAction


update : Msg -> Model -> ( Model, Action )
update msg model =
    case msg of
        OpenCreateForm ->
            ( model, RequestCreateForm (nextPort model) )

        DeleteServer id ->
            ( model, RequestDeleteServer id )

        AddClient serverId ->
            ( { model | expandedClients = Set.insert serverId model.expandedClients }
            , RequestAddClient serverId
            )

        DeleteClient clientId ->
            ( model, RequestDeleteClient clientId )

        ImportClient server client ->
            ( model
            , RequestImportClient
                { name = server.name ++ " - " ++ client.clientId
                , issuerUrl = server.issuerUrl
                , authorizationUrl = server.authorizationUrl
                , tokenUrl = server.tokenEndpoint
                , clientId = client.clientId
                , clientSecret = client.clientSecret
                }
            )

        StartServer id ->
            ( model, RequestStartServer id )

        StopServer id ->
            ( model, RequestStopServer id )

        ToggleClients serverId ->
            let
                newExpanded =
                    if Set.member serverId model.expandedClients then
                        Set.remove serverId model.expandedClients

                    else
                        Set.insert serverId model.expandedClients
            in
            ( { model | expandedClients = newExpanded }, NoAction )

        GotServerConfigs result ->
            case result of
                Ok configs ->
                    ( { model | servers = configs }, NoAction )

                Err _ ->
                    ( model, NoAction )



-- PORTS / HELPERS


nextPort : Model -> Int
nextPort model =
    let
        basePort =
            9500

        usedPorts =
            List.map .port_ model.servers
    in
    findAvailable basePort usedPorts


findAvailable : Int -> List Int -> Int
findAvailable candidate used =
    if List.member candidate used then
        findAvailable (candidate + 1) used

    else
        candidate



-- DECODERS


clientDecoder : Decode.Decoder Client
clientDecoder =
    Decode.map3 Client
        (Decode.field "id" Decode.int |> Decode.map String.fromInt)
        (Decode.field "clientId" Decode.string)
        (Decode.field "clientSecret" Decode.string)


serverConfigDecoder : Decode.Decoder OAuthServer
serverConfigDecoder =
    Decode.map6
        (\id name authUrl tokUrl running clients ->
            let
                port_ =
                    extractPort authUrl
            in
            { id = String.fromInt id
            , name = name
            , issuerUrl = "http://localhost:" ++ String.fromInt port_
            , authorizationUrl = authUrl
            , tokenEndpoint = tokUrl
            , port_ = port_
            , running = running
            , clients = clients
            }
        )
        (Decode.field "id" Decode.int)
        (Decode.field "configName" Decode.string)
        (Decode.field "authServerUrl" Decode.string)
        (Decode.field "tokenUrl" Decode.string)
        (Decode.field "running" Decode.bool)
        (Decode.field "clients" (Decode.list clientDecoder))


extractPort : String -> Int
extractPort url =
    url
        |> String.replace "http://localhost:" ""
        |> String.split "/"
        |> List.head
        |> Maybe.andThen String.toInt
        |> Maybe.withDefault 9500



-- VIEW


view : Model -> Html Msg
view model =
    div [ class "page-content" ]
        [ viewBody model
        , viewFooter
        ]


viewBody : Model -> Html Msg
viewBody model =
    if List.isEmpty model.servers then
        div [ class "empty-state" ]
            [ span [ class "empty-label" ] [ text "No servers configured" ] ]

    else
        div [ class "server-list" ]
            (List.map (viewServerCard model.expandedClients) model.servers)


viewServerCard : Set String -> OAuthServer -> Html Msg
viewServerCard expandedClients server =
    let
        isExpanded =
            Set.member server.id expandedClients

        clientCount =
            List.length server.clients

        chevron =
            if isExpanded then
                "\u{25BC}"

            else
                "\u{25B6}"
    in
    div [ class "server-card" ]
        [ div [ class "server-card-header" ]
            [ div [ class "server-card-left" ]
                [ span [ class "server-name" ] [ text server.name ]
                , span [ class "server-meta" ] [ text ("localhost:" ++ String.fromInt server.port_) ]
                ]
            , div [ class "server-card-right" ]
                [ viewServerControls server
                , div [ class ("status-dot" ++ statusClass server.running) ] []
                , span [ class "status-label" ] [ text (statusLabel server.running) ]
                , button
                    [ class "btn-delete"
                    , onClick (DeleteServer server.id)
                    , disabled server.running
                    ]
                    [ text "\u{1F5D1}" ]
                ]
            ]
        , div [ class "server-card-details" ]
            [ viewDetail "Issuer" server.issuerUrl
            , viewDetail "Authorization" server.authorizationUrl
            , viewDetail "Token" server.tokenEndpoint
            ]
        , div [ class "client-section" ]
            [ div [ class "client-section-header" ]
                [ span [ class "expandable-toggle", onClick (ToggleClients server.id) ]
                    [ text (chevron ++ " Clients (" ++ String.fromInt clientCount ++ ")") ]
                , button [ class "btn-add-client", onClick (AddClient server.id) ] [ text "+ Add Client" ]
                ]
            , if isExpanded then
                if List.isEmpty server.clients then
                    div [ class "client-empty" ] [ text "No clients" ]

                else
                    div [ class "client-list" ]
                        (List.map (viewClientCard server) server.clients)

              else
                text ""
            ]
        ]


viewServerControls : OAuthServer -> Html Msg
viewServerControls server =
    if server.running then
        button [ class "btn-server-control btn-stop", onClick (StopServer server.id) ]
            [ text "\u{25A0} Stop" ]

    else
        button [ class "btn-server-control btn-start", onClick (StartServer server.id) ]
            [ text "\u{25B6} Start" ]


viewClientCard : OAuthServer -> Client -> Html Msg
viewClientCard server client =
    div [ class "client-card" ]
        [ div [ class "client-card-header" ]
            [ span [ class "client-label" ] [ text ("Client " ++ client.id) ]
            , div [ class "client-card-actions" ]
                [ button [ class "btn-import", onClick (ImportClient server client) ] [ text "Import" ]
                , button [ class "btn-delete", onClick (DeleteClient client.id) ] [ text "\u{1F5D1}" ]
                ]
            ]
        , div [ class "client-details" ]
            [ viewDetail "Client ID" client.clientId
            , viewDetail "Secret" client.clientSecret
            ]
        ]


viewDetail : String -> String -> Html Msg
viewDetail label value =
    div [ class "server-detail" ]
        [ span [ class "detail-label" ] [ text label ]
        , span [ class "detail-value" ] [ text value ]
        ]


statusClass : Bool -> String
statusClass running =
    if running then
        " running"

    else
        ""


statusLabel : Bool -> String
statusLabel running =
    if running then
        "running"

    else
        "stopped"


viewFooter : Html Msg
viewFooter =
    div [ class "footer" ]
        [ button [ class "btn-create", onClick OpenCreateForm ]
            [ text "+ Create new OAuth Server" ]
        ]
