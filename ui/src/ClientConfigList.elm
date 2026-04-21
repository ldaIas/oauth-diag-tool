-- Copyright 2026 Sam Sovereign
-- SPDX-License-Identifier: Apache-2.0


module ClientConfigList exposing (Action(..), AuthResult, ClientConfig, MetadataResult, Model, Msg(..), authResultDecoder, clientConfigDecoder, metadataResultDecoder, init, update, view)

import Dict exposing (Dict)
import Html exposing (..)
import Html.Attributes exposing (checked, class, type_)
import Html.Events exposing (onClick)
import Json.Decode as Decode
import Set exposing (Set)


-- MODEL


type alias ClientConfig =
    { id : String
    , name : String
    , issuerUrl : String
    , authorizationUrl : String
    , tokenUrl : String
    , clientId : String
    , clientSecret : String
    , scopes : String
    , grantType : String
    , extraParams : String
    , disabledParams : String
    , disabledTokenParams : String
    , scopesSupported : String
    , refreshToken : String
    , disabledRefreshParams : String
    }


type alias MetadataResult =
    { configId : String
    , body : String
    , authorizationEndpoint : String
    , tokenEndpoint : String
    , scopesSupported : String
    }


type alias AuthResult =
    { configId : String
    , statusCode : Int
    , headers : List ( String, String )
    , body : String
    }


type AuthState
    = Idle
    | Loading
    | Done AuthResult
    | Error String


type MetadataState
    = MetadataIdle
    | MetadataLoading
    | MetadataDone String
    | MetadataError String


type alias Model =
    { configs : List ClientConfig
    , authStates : Dict String AuthState
    , expandedResults : Set String
    , expandedParams : Set String
    , expandedTokenParams : Set String
    , expandedRefreshParams : Set String
    , expandedMetadata : Set String
    , metadataStates : Dict String MetadataState
    }


init : Model
init =
    { configs = []
    , authStates = Dict.empty
    , expandedResults = Set.empty
    , expandedParams = Set.empty
    , expandedTokenParams = Set.empty
    , expandedRefreshParams = Set.empty
    , expandedMetadata = Set.empty
    , metadataStates = Dict.empty
    }



-- UPDATE


type Msg
    = OpenCreateForm
    | DeleteConfig String
    | EditConfig ClientConfig
    | AuthorizeConfig String
    | CancelAuthorize String
    | ToggleResults String
    | ToggleParam String String
    | ToggleTokenParam String String
    | ToggleParamsSection String
    | ToggleTokenParamsSection String
    | ToggleRefreshParam String String
    | ToggleRefreshParamsSection String
    | ToggleMetadataSection String
    | FetchMetadata String String
    | GotMetadataResult (Result Decode.Error MetadataResult)
    | RefreshTokenConfig String
    | GotAuthResult (Result Decode.Error AuthResult)
    | GotClientConfigs (Result Decode.Error (List ClientConfig))


type Action
    = RequestCreateForm
    | RequestDeleteConfig String
    | RequestEditConfig ClientConfig
    | RequestAuthorizeConfig String
    | RequestCancelAuthorize String
    | RequestUpdateConfig ClientConfig
    | RequestRefreshToken String
    | RequestFetchMetadata String String
    | NoAction


update : Msg -> Model -> ( Model, Action )
update msg model =
    case msg of
        OpenCreateForm ->
            ( model, RequestCreateForm )

        DeleteConfig id ->
            ( model, RequestDeleteConfig id )

        EditConfig config ->
            ( model, RequestEditConfig config )

        AuthorizeConfig id ->
            ( { model
                | authStates = Dict.insert id Loading model.authStates
                , expandedResults = Set.insert id model.expandedResults
              }
            , RequestAuthorizeConfig id
            )

        RefreshTokenConfig id ->
            ( { model
                | authStates = Dict.insert id Loading model.authStates
                , expandedResults = Set.insert id model.expandedResults
              }
            , RequestRefreshToken id
            )

        CancelAuthorize id ->
            ( { model | authStates = Dict.insert id Idle model.authStates }
            , RequestCancelAuthorize id
            )

        ToggleResults id ->
            let
                newExpanded =
                    if Set.member id model.expandedResults then
                        Set.remove id model.expandedResults

                    else
                        Set.insert id model.expandedResults
            in
            ( { model | expandedResults = newExpanded }, NoAction )

        ToggleParamsSection id ->
            let
                newExpanded =
                    if Set.member id model.expandedParams then
                        Set.remove id model.expandedParams

                    else
                        Set.insert id model.expandedParams
            in
            ( { model | expandedParams = newExpanded }, NoAction )

        ToggleTokenParamsSection id ->
            let
                newExpanded =
                    if Set.member id model.expandedTokenParams then
                        Set.remove id model.expandedTokenParams

                    else
                        Set.insert id model.expandedTokenParams
            in
            ( { model | expandedTokenParams = newExpanded }, NoAction )

        ToggleParam configId paramName ->
            let
                updatedConfigs =
                    List.map
                        (\config ->
                            if config.id == configId then
                                { config | disabledParams = toggleDisabledParam paramName config.disabledParams }

                            else
                                config
                        )
                        model.configs

                updatedConfig =
                    List.filter (\c -> c.id == configId) updatedConfigs
                        |> List.head
            in
            case updatedConfig of
                Just config ->
                    ( { model | configs = updatedConfigs }, RequestUpdateConfig config )

                Nothing ->
                    ( model, NoAction )

        ToggleTokenParam configId paramName ->
            let
                updatedConfigs =
                    List.map
                        (\config ->
                            if config.id == configId then
                                { config | disabledTokenParams = toggleDisabledParam paramName config.disabledTokenParams }

                            else
                                config
                        )
                        model.configs

                updatedConfig =
                    List.filter (\c -> c.id == configId) updatedConfigs
                        |> List.head
            in
            case updatedConfig of
                Just config ->
                    ( { model | configs = updatedConfigs }, RequestUpdateConfig config )

                Nothing ->
                    ( model, NoAction )

        ToggleRefreshParam configId paramName ->
            let
                updatedConfigs =
                    List.map
                        (\config ->
                            if config.id == configId then
                                { config | disabledRefreshParams = toggleDisabledParam paramName config.disabledRefreshParams }

                            else
                                config
                        )
                        model.configs

                updatedConfig =
                    List.filter (\c -> c.id == configId) updatedConfigs
                        |> List.head
            in
            case updatedConfig of
                Just config ->
                    ( { model | configs = updatedConfigs }, RequestUpdateConfig config )

                Nothing ->
                    ( model, NoAction )

        ToggleRefreshParamsSection id ->
            let
                newExpanded =
                    if Set.member id model.expandedRefreshParams then
                        Set.remove id model.expandedRefreshParams

                    else
                        Set.insert id model.expandedRefreshParams
            in
            ( { model | expandedRefreshParams = newExpanded }, NoAction )

        ToggleMetadataSection id ->
            let
                newExpanded =
                    if Set.member id model.expandedMetadata then
                        Set.remove id model.expandedMetadata

                    else
                        Set.insert id model.expandedMetadata
            in
            ( { model | expandedMetadata = newExpanded }, NoAction )

        FetchMetadata configId issuerUrl ->
            ( { model
                | metadataStates = Dict.insert configId MetadataLoading model.metadataStates
                , expandedMetadata = Set.insert configId model.expandedMetadata
              }
            , RequestFetchMetadata configId issuerUrl
            )

        GotMetadataResult result ->
            case result of
                Ok metadata ->
                    if String.isEmpty metadata.authorizationEndpoint && String.isEmpty metadata.tokenEndpoint then
                        -- Error response: endpoints are empty, body contains the error
                        ( { model | metadataStates = Dict.insert metadata.configId (MetadataError metadata.body) model.metadataStates }
                        , NoAction
                        )

                    else
                        let
                            updatedConfigs =
                                List.map
                                    (\config ->
                                        if config.id == metadata.configId then
                                            { config
                                                | authorizationUrl = metadata.authorizationEndpoint
                                                , tokenUrl = metadata.tokenEndpoint
                                                , scopesSupported = metadata.scopesSupported
                                            }

                                        else
                                            config
                                    )
                                    model.configs

                            updatedConfig =
                                List.filter (\c -> c.id == metadata.configId) updatedConfigs
                                    |> List.head
                        in
                        case updatedConfig of
                            Just config ->
                                ( { model
                                    | configs = updatedConfigs
                                    , metadataStates = Dict.insert metadata.configId (MetadataDone metadata.body) model.metadataStates
                                  }
                                , RequestUpdateConfig config
                                )

                            Nothing ->
                                ( { model | metadataStates = Dict.insert metadata.configId (MetadataDone metadata.body) model.metadataStates }
                                , NoAction
                                )

                Err _ ->
                    ( model, NoAction )

        GotAuthResult result ->
            case result of
                Ok authResult ->
                    let
                        maybeRefreshToken =
                            case Decode.decodeString (Decode.field "refresh_token" Decode.string) authResult.body of
                                Ok rt ->
                                    Just rt

                                Err _ ->
                                    Nothing

                        updatedConfigs =
                            case maybeRefreshToken of
                                Just rt ->
                                    List.map
                                        (\c ->
                                            if c.id == authResult.configId then
                                                { c | refreshToken = rt }

                                            else
                                                c
                                        )
                                        model.configs

                                Nothing ->
                                    model.configs
                    in
                    ( { model
                        | authStates = Dict.insert authResult.configId (Done authResult) model.authStates
                        , expandedResults = Set.insert authResult.configId model.expandedResults
                        , configs = updatedConfigs
                      }
                    , NoAction
                    )

                Err _ ->
                    ( model, NoAction )

        GotClientConfigs result ->
            case result of
                Ok configs ->
                    ( { model | configs = configs }, NoAction )

                Err _ ->
                    ( model, NoAction )



-- DECODERS


clientConfigDecoder : Decode.Decoder ClientConfig
clientConfigDecoder =
    Decode.map8 ClientConfig
        (Decode.field "id" Decode.int |> Decode.map String.fromInt)
        (Decode.field "name" Decode.string)
        (Decode.field "issuerUrl" Decode.string)
        (Decode.field "authorizationUrl" Decode.string)
        (Decode.field "tokenUrl" Decode.string)
        (Decode.field "clientId" Decode.string)
        (Decode.field "clientSecret" Decode.string)
        (Decode.field "scopes" Decode.string)
        |> andMap (Decode.field "grantType" Decode.string)
        |> andMap (Decode.field "extraParams" Decode.string)
        |> andMap (Decode.field "disabledParams" Decode.string)
        |> andMap (Decode.field "disabledTokenParams" Decode.string)
        |> andMap (Decode.field "scopesSupported" Decode.string)
        |> andMap (Decode.field "refreshToken" Decode.string)
        |> andMap (Decode.field "disabledRefreshParams" Decode.string)


metadataResultDecoder : Decode.Decoder MetadataResult
metadataResultDecoder =
    Decode.map5 MetadataResult
        (Decode.field "configId" Decode.string)
        (Decode.field "body" Decode.string)
        (Decode.field "authorizationEndpoint" Decode.string)
        (Decode.field "tokenEndpoint" Decode.string)
        (Decode.field "scopesSupported" Decode.string)


authResultDecoder : Decode.Decoder AuthResult
authResultDecoder =
    Decode.map4 AuthResult
        (Decode.field "configId" Decode.string)
        (Decode.field "statusCode" Decode.int)
        (Decode.field "headers" (Decode.list headerDecoder))
        (Decode.field "body" Decode.string)


headerDecoder : Decode.Decoder ( String, String )
headerDecoder =
    Decode.map2 Tuple.pair
        (Decode.index 0 Decode.string)
        (Decode.index 1 Decode.string)


andMap : Decode.Decoder a -> Decode.Decoder (a -> b) -> Decode.Decoder b
andMap =
    Decode.map2 (|>)



-- DISABLED PARAMS HELPERS


isParamDisabled : String -> String -> Bool
isParamDisabled paramName disabledParams =
    String.contains ("\"" ++ paramName ++ "\":true") disabledParams


toggleDisabledParam : String -> String -> String
toggleDisabledParam paramName disabledParams =
    let
        decoded : Dict String Bool
        decoded =
            case Decode.decodeString (Decode.dict Decode.bool) disabledParams of
                Ok dict ->
                    dict

                Err _ ->
                    Dict.empty

        updated =
            if Dict.member paramName decoded then
                Dict.remove paramName decoded

            else
                Dict.insert paramName True decoded

        entries =
            Dict.toList updated
                |> List.map (\( k, _ ) -> "\"" ++ k ++ "\":true")
    in
    if List.isEmpty entries then
        "{}"

    else
        "{" ++ String.join "," entries ++ "}"



-- VIEW


view : String -> Model -> Html Msg
view callbackUrl model =
    div [ class "page-content" ]
        [ viewBody callbackUrl model
        , viewFooter
        ]


viewBody : String -> Model -> Html Msg
viewBody callbackUrl model =
    if List.isEmpty model.configs then
        div [ class "empty-state" ]
            [ span [ class "empty-label" ] [ text "No client configurations" ] ]

    else
        div [ class "server-list" ]
            (List.map (viewConfigCard callbackUrl model.authStates model.expandedResults model.expandedParams model.expandedTokenParams model.expandedRefreshParams model.expandedMetadata model.metadataStates) model.configs)


viewConfigCard : String -> Dict String AuthState -> Set String -> Set String -> Set String -> Set String -> Set String -> Dict String MetadataState -> ClientConfig -> Html Msg
viewConfigCard callbackUrl authStates expandedResults expandedParams expandedTokenParams expandedRefreshParams expandedMetadata metadataStates config =
    let
        state =
            Dict.get config.id authStates |> Maybe.withDefault Idle

        isExpanded =
            Set.member config.id expandedResults

        hasResult =
            case state of
                Idle ->
                    False

                _ ->
                    True

        chevron =
            if isExpanded then
                "\u{25BC}"

            else
                "\u{25B6}"
    in
    div [ class "server-card" ]
        [ div [ class "server-card-header" ]
            [ div [ class "server-card-left" ]
                [ span [ class "server-name" ] [ text config.name ]
                , span [ class "server-meta" ] [ text config.grantType ]
                ]
            , div [ class "server-card-right" ]
                [ viewAuthorizeButton state config
                , button [ class "btn-import", onClick (EditConfig config) ] [ text "Edit" ]
                , button [ class "btn-delete", onClick (DeleteConfig config.id) ] [ text "\u{1F5D1}" ]
                ]
            ]
        , div [ class "server-card-details" ]
            [ viewDetail "Issuer" config.issuerUrl
            , viewDetail "Authorization" config.authorizationUrl
            , viewDetail "Token" config.tokenUrl
            , viewDetail "Client ID" config.clientId
            , viewDetail "Secret" config.clientSecret
            , if config.grantType == "authorization_code" && not (String.isEmpty callbackUrl) then
                viewDetail "Callback URL" callbackUrl

              else
                text ""
            , if String.isEmpty config.scopes then
                text ""

              else
                viewDetail "Scopes" config.scopes
            , if String.isEmpty config.extraParams then
                text ""

              else
                viewDetail "Extra Params" config.extraParams
            , if String.isEmpty config.scopesSupported then
                text ""

              else
                viewDetail "Scopes Supported" config.scopesSupported
            ]
        , if config.grantType == "authorization_code" then
            div []
                [ viewAuthorizeParamToggles expandedParams config
                , viewTokenParamToggles expandedTokenParams config
                ]

          else
            viewParamToggles expandedParams config
        , if not (String.isEmpty config.refreshToken) then
            viewRefreshParamToggles expandedRefreshParams config

          else
            text ""
        , viewMetadataSection expandedMetadata metadataStates config
        , if hasResult then
            div [ class "results-section" ]
                [ span [ class "expandable-toggle", onClick (ToggleResults config.id) ]
                    [ text (chevron ++ " Results") ]
                , if isExpanded then
                    viewAuthResponse state

                  else
                    text ""
                ]

          else
            text ""
        ]


viewMetadataSection : Set String -> Dict String MetadataState -> ClientConfig -> Html Msg
viewMetadataSection expandedMetadata metadataStates config =
    let
        isExpanded =
            Set.member config.id expandedMetadata

        metaState =
            Dict.get config.id metadataStates |> Maybe.withDefault MetadataIdle

        chevron =
            if isExpanded then
                "\u{25BC}"

            else
                "\u{25B6}"
    in
    div [ class "param-toggles" ]
        [ span [ class "expandable-toggle", onClick (ToggleMetadataSection config.id) ]
            [ text (chevron ++ " Authorization Server Metadata") ]
        , if isExpanded then
            div [ class "metadata-section-content" ]
                [ button [ class "btn-server-control btn-authorize", onClick (FetchMetadata config.id config.issuerUrl) ]
                    [ text "Refresh Metadata" ]
                , viewMetadataState metaState
                ]

          else
            text ""
        ]


viewMetadataState : MetadataState -> Html Msg
viewMetadataState state =
    case state of
        MetadataIdle ->
            div [ class "metadata-hint" ]
                [ span [ class "auth-loading" ] [ text "Click Refresh Metadata to fetch" ] ]

        MetadataLoading ->
            div [ class "metadata-hint" ]
                [ span [ class "auth-loading" ] [ text "Fetching metadata..." ] ]

        MetadataDone body ->
            div [ class "auth-response-body" ]
                [ span [ class "detail-label" ] [ text "Response" ]
                , pre [ class "auth-body-content" ] [ text body ]
                ]

        MetadataError err ->
            div [ class "auth-response-error" ]
                [ span [ class "detail-label" ] [ text "Error" ]
                , span [ class "auth-error-text" ] [ text err ]
                ]


viewParamToggles : Set String -> ClientConfig -> Html Msg
viewParamToggles expandedParams config =
    let
        isExpanded =
            Set.member config.id expandedParams

        chevron =
            if isExpanded then
                "\u{25BC}"

            else
                "\u{25B6}"

        standardParams =
            [ "client_id", "client_secret", "scope" ]

        authCodeParams =
            if config.grantType == "authorization_code" then
                [ "response_type", "redirect_uri", "state" ]

            else
                []

        pkceParams =
            if config.grantType == "authorization_code" then
                [ "code_challenge", "code_challenge_method", "code_verifier" ]

            else
                []

        extraParamKeys =
            case Decode.decodeString (Decode.dict Decode.string) config.extraParams of
                Ok dict ->
                    Dict.keys dict

                Err _ ->
                    []

        allParams =
            standardParams ++ authCodeParams ++ pkceParams ++ extraParamKeys
    in
    div [ class "param-toggles" ]
        [ span [ class "expandable-toggle", onClick (ToggleParamsSection config.id) ]
            [ text (chevron ++ " Send Parameters") ]
        , if isExpanded then
            div [ class "param-toggle-list" ]
                (List.map (viewParamToggle ToggleParam config.id config.disabledParams) allParams)

          else
            text ""
        ]


viewAuthorizeParamToggles : Set String -> ClientConfig -> Html Msg
viewAuthorizeParamToggles expandedParams config =
    let
        isExpanded =
            Set.member config.id expandedParams

        chevron =
            if isExpanded then
                "\u{25BC}"

            else
                "\u{25B6}"

        authorizeParams =
            [ "response_type", "client_id", "redirect_uri", "state", "scope", "code_challenge", "code_challenge_method" ]

        extraParamKeys =
            case Decode.decodeString (Decode.dict Decode.string) config.extraParams of
                Ok dict ->
                    Dict.keys dict

                Err _ ->
                    []

        allParams =
            authorizeParams ++ extraParamKeys
    in
    div [ class "param-toggles" ]
        [ span [ class "expandable-toggle", onClick (ToggleParamsSection config.id) ]
            [ text (chevron ++ " Authorize Parameters") ]
        , if isExpanded then
            div [ class "param-toggle-list" ]
                (List.map (viewParamToggle ToggleParam config.id config.disabledParams) allParams)

          else
            text ""
        ]


viewTokenParamToggles : Set String -> ClientConfig -> Html Msg
viewTokenParamToggles expandedTokenParams config =
    let
        isExpanded =
            Set.member config.id expandedTokenParams

        chevron =
            if isExpanded then
                "\u{25BC}"

            else
                "\u{25B6}"

        tokenParams =
            [ "client_id", "client_secret", "redirect_uri", "code_verifier" ]

        extraParamKeys =
            case Decode.decodeString (Decode.dict Decode.string) config.extraParams of
                Ok dict ->
                    Dict.keys dict

                Err _ ->
                    []

        allParams =
            tokenParams ++ extraParamKeys
    in
    div [ class "param-toggles" ]
        [ span [ class "expandable-toggle", onClick (ToggleTokenParamsSection config.id) ]
            [ text (chevron ++ " Token Parameters") ]
        , if isExpanded then
            div [ class "param-toggle-list" ]
                (List.map (viewParamToggle ToggleTokenParam config.id config.disabledTokenParams) allParams)

          else
            text ""
        ]


viewRefreshParamToggles : Set String -> ClientConfig -> Html Msg
viewRefreshParamToggles expandedRefreshParams config =
    let
        isExpanded =
            Set.member config.id expandedRefreshParams

        chevron =
            if isExpanded then
                "\u{25BC}"

            else
                "\u{25B6}"

        refreshParams =
            [ "grant_type", "refresh_token", "client_id", "client_secret", "scope" ]
    in
    div [ class "param-toggles" ]
        [ span [ class "expandable-toggle", onClick (ToggleRefreshParamsSection config.id) ]
            [ text (chevron ++ " Refresh Token Parameters") ]
        , if isExpanded then
            div [ class "param-toggle-list" ]
                (List.map (viewParamToggle ToggleRefreshParam config.id config.disabledRefreshParams) refreshParams)

          else
            text ""
        ]


viewParamToggle : (String -> String -> Msg) -> String -> String -> String -> Html Msg
viewParamToggle toggleMsg configId disabledParams paramName =
    let
        disabled =
            isParamDisabled paramName disabledParams
    in
    label [ class "param-toggle" ]
        [ input
            [ type_ "checkbox"
            , checked (not disabled)
            , onClick (toggleMsg configId paramName)
            ]
            []
        , span [ class "param-toggle-name" ] [ text paramName ]
        ]


viewAuthorizeButton : AuthState -> ClientConfig -> Html Msg
viewAuthorizeButton state config =
    case state of
        Loading ->
            span []
                [ button [ class "btn-server-control btn-authorize", Html.Attributes.disabled True ]
                    [ text "Authorizing..." ]
                , button [ class "btn-server-control btn-cancel-auth", onClick (CancelAuthorize config.id) ]
                    [ text "Cancel" ]
                ]

        _ ->
            span []
                [ button [ class "btn-server-control btn-authorize", onClick (AuthorizeConfig config.id) ]
                    [ text "Authorize" ]
                , if not (String.isEmpty config.refreshToken) then
                    button [ class "btn-server-control btn-refresh-token", onClick (RefreshTokenConfig config.id) ]
                        [ text "Refresh Token" ]

                  else
                    text ""
                ]


viewAuthResponse : AuthState -> Html Msg
viewAuthResponse state =
    case state of
        Idle ->
            text ""

        Loading ->
            div [ class "auth-response" ]
                [ span [ class "auth-loading" ] [ text "Requesting token..." ] ]

        Done result ->
            div [ class "auth-response" ]
                [ div [ class "auth-response-status" ]
                    [ span [ class "detail-label" ] [ text "Status" ]
                    , span [ class (statusClass result.statusCode) ] [ text (String.fromInt result.statusCode) ]
                    ]
                , div [ class "auth-response-headers" ]
                    [ span [ class "detail-label" ] [ text "Headers" ]
                    , div [ class "auth-headers-list" ]
                        (List.map viewHeader result.headers)
                    ]
                , div [ class "auth-response-body" ]
                    [ span [ class "detail-label" ] [ text "Body" ]
                    , pre [ class "auth-body-content" ] [ text result.body ]
                    ]
                ]

        Error err ->
            div [ class "auth-response" ]
                [ div [ class "auth-response-error" ]
                    [ span [ class "detail-label" ] [ text "Error" ]
                    , span [ class "auth-error-text" ] [ text err ]
                    ]
                ]


statusClass : Int -> String
statusClass code =
    if code >= 200 && code < 300 then
        "detail-value auth-status-ok"

    else
        "detail-value auth-status-error"


viewHeader : ( String, String ) -> Html Msg
viewHeader ( key, value ) =
    div [ class "auth-header-item" ]
        [ span [ class "auth-header-key" ] [ text key ]
        , span [ class "auth-header-value" ] [ text value ]
        ]


viewDetail : String -> String -> Html Msg
viewDetail label value =
    div [ class "server-detail" ]
        [ span [ class "detail-label" ] [ text label ]
        , span [ class "detail-value" ] [ text value ]
        ]


viewFooter : Html Msg
viewFooter =
    div [ class "footer" ]
        [ button [ class "btn-create", onClick OpenCreateForm ]
            [ text "+ Add Client Configuration" ]
        ]
