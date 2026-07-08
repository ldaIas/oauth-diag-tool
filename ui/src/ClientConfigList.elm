-- Copyright 2026 Sam Sovereign
-- SPDX-License-Identifier: Apache-2.0


module ClientConfigList exposing
    ( Action(..)
    , AuthResult
    , ClientConfig
    , MetadataPayload
    , MetadataResult
    , Model
    , Msg(..)
    , authResultDecoder
    , clientConfigDecoder
    , encodeConfigFields
    , init
    , metadataResultDecoder
    , update
    , view
    )

import Dict exposing (Dict)
import Html exposing (..)
import Html.Attributes exposing (checked, class, type_)
import Html.Events exposing (onClick)
import Json.Decode as Decode
import Json.Encode as Encode
import Set exposing (Set)


-- MODEL


type alias ClientConfig =
    { id : Int
    , name : String
    , issuerUrl : String
    , authorizationUrl : String
    , tokenUrl : String
    , clientId : String
    , clientSecret : String
    , scopes : String
    , grantType : String
    , extraParams : Dict String String
    , disabledParams : Dict String Bool
    , disabledTokenParams : Dict String Bool
    , scopesSupported : String
    , refreshToken : String
    , disabledRefreshParams : Dict String Bool
    }


type alias MetadataPayload =
    { body : String
    , authorizationEndpoint : String
    , tokenEndpoint : String
    , scopesSupported : String
    }


type alias MetadataResult =
    { configId : Maybe Int
    , outcome : Result String MetadataPayload
    }


type alias AuthSuccessData =
    { statusCode : Int
    , headers : List ( String, String )
    , body : String
    }


type AuthOutcome
    = AuthSuccess AuthSuccessData
    | AuthFailure String


type alias AuthResult =
    { configId : Int
    , outcome : AuthOutcome
    }


type AuthState
    = Idle
    | Loading
    | Done AuthSuccessData
    | Error String


type MetadataState
    = MetadataIdle
    | MetadataLoading
    | MetadataDone String
    | MetadataError String


type alias Model =
    { configs : List ClientConfig
    , authStates : Dict Int AuthState
    , expandedResults : Set Int
    , expandedParams : Set Int
    , expandedTokenParams : Set Int
    , expandedRefreshParams : Set Int
    , expandedMetadata : Set Int
    , metadataStates : Dict Int MetadataState
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
    | DeleteConfig Int
    | EditConfig ClientConfig
    | AuthorizeConfig Int
    | CancelAuthorize Int
    | ToggleResults Int
    | ToggleParam Int String
    | ToggleTokenParam Int String
    | ToggleRefreshParam Int String
    | ToggleParamsSection Int
    | ToggleTokenParamsSection Int
    | ToggleRefreshParamsSection Int
    | ToggleMetadataSection Int
    | FetchMetadata Int String
    | GotMetadataResult MetadataResult
    | RefreshTokenConfig Int
    | GotAuthResult AuthResult
    | GotClientConfigs (List ClientConfig)


type Action
    = RequestCreateForm
    | RequestDeleteConfig Int
    | RequestEditConfig ClientConfig
    | RequestAuthorizeConfig Int
    | RequestCancelAuthorize Int
    | RequestUpdateConfig ClientConfig
    | RequestRefreshToken Int
    | RequestFetchMetadata Int String
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
            ( { model | expandedResults = toggleMember id model.expandedResults }, NoAction )

        ToggleParamsSection id ->
            ( { model | expandedParams = toggleMember id model.expandedParams }, NoAction )

        ToggleTokenParamsSection id ->
            ( { model | expandedTokenParams = toggleMember id model.expandedTokenParams }, NoAction )

        ToggleRefreshParamsSection id ->
            ( { model | expandedRefreshParams = toggleMember id model.expandedRefreshParams }, NoAction )

        ToggleMetadataSection id ->
            ( { model | expandedMetadata = toggleMember id model.expandedMetadata }, NoAction )

        ToggleParam configId paramName ->
            updateConfigAndSave configId
                (\c -> { c | disabledParams = toggleDisabled paramName c.disabledParams })
                model

        ToggleTokenParam configId paramName ->
            updateConfigAndSave configId
                (\c -> { c | disabledTokenParams = toggleDisabled paramName c.disabledTokenParams })
                model

        ToggleRefreshParam configId paramName ->
            updateConfigAndSave configId
                (\c -> { c | disabledRefreshParams = toggleDisabled paramName c.disabledRefreshParams })
                model

        FetchMetadata configId issuerUrl ->
            ( { model
                | metadataStates = Dict.insert configId MetadataLoading model.metadataStates
                , expandedMetadata = Set.insert configId model.expandedMetadata
              }
            , RequestFetchMetadata configId issuerUrl
            )

        GotMetadataResult result ->
            case result.configId of
                Nothing ->
                    -- Form-submission fetches are handled by Main
                    ( model, NoAction )

                Just configId ->
                    case result.outcome of
                        Err err ->
                            ( { model | metadataStates = Dict.insert configId (MetadataError err) model.metadataStates }
                            , NoAction
                            )

                        Ok payload ->
                            updateConfigAndSave configId
                                (\c ->
                                    { c
                                        | authorizationUrl = payload.authorizationEndpoint
                                        , tokenUrl = payload.tokenEndpoint
                                        , scopesSupported = payload.scopesSupported
                                    }
                                )
                                { model | metadataStates = Dict.insert configId (MetadataDone payload.body) model.metadataStates }

        GotAuthResult authResult ->
            case authResult.outcome of
                AuthFailure err ->
                    ( { model
                        | authStates = Dict.insert authResult.configId (Error err) model.authStates
                        , expandedResults = Set.insert authResult.configId model.expandedResults
                      }
                    , NoAction
                    )

                AuthSuccess data ->
                    let
                        maybeRefreshToken =
                            Decode.decodeString (Decode.field "refresh_token" Decode.string) data.body
                                |> Result.toMaybe

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
                        | authStates = Dict.insert authResult.configId (Done data) model.authStates
                        , expandedResults = Set.insert authResult.configId model.expandedResults
                        , configs = updatedConfigs
                      }
                    , NoAction
                    )

        GotClientConfigs configs ->
            ( { model | configs = configs }, NoAction )


toggleMember : Int -> Set Int -> Set Int
toggleMember id set =
    if Set.member id set then
        Set.remove id set

    else
        Set.insert id set


{-| Apply an update to one config and request that it be persisted.
-}
updateConfigAndSave : Int -> (ClientConfig -> ClientConfig) -> Model -> ( Model, Action )
updateConfigAndSave configId updateFn model =
    let
        updatedConfigs =
            List.map
                (\c ->
                    if c.id == configId then
                        updateFn c

                    else
                        c
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



-- DECODERS


clientConfigDecoder : Decode.Decoder ClientConfig
clientConfigDecoder =
    Decode.map8 ClientConfig
        (Decode.field "id" Decode.int)
        (Decode.field "name" Decode.string)
        (Decode.field "issuerUrl" Decode.string)
        (Decode.field "authorizationUrl" Decode.string)
        (Decode.field "tokenUrl" Decode.string)
        (Decode.field "clientId" Decode.string)
        (Decode.field "clientSecret" Decode.string)
        (Decode.field "scopes" Decode.string)
        |> andMap (Decode.field "grantType" Decode.string)
        |> andMap (Decode.field "extraParams" (jsonStringDict Decode.string))
        |> andMap (Decode.field "disabledParams" (jsonStringDict Decode.bool))
        |> andMap (Decode.field "disabledTokenParams" (jsonStringDict Decode.bool))
        |> andMap (Decode.field "scopesSupported" Decode.string)
        |> andMap (Decode.field "refreshToken" Decode.string)
        |> andMap (Decode.field "disabledRefreshParams" (jsonStringDict Decode.bool))


{-| The backend stores these maps as JSON text inside a string column; decode
that string into a proper Dict at the boundary.
-}
jsonStringDict : Decode.Decoder a -> Decode.Decoder (Dict String a)
jsonStringDict valueDecoder =
    Decode.string
        |> Decode.map
            (\raw ->
                if String.isEmpty raw then
                    Dict.empty

                else
                    Decode.decodeString (Decode.dict valueDecoder) raw
                        |> Result.withDefault Dict.empty
            )


metadataResultDecoder : Decode.Decoder MetadataResult
metadataResultDecoder =
    Decode.map2 MetadataResult
        (Decode.field "configId" (Decode.nullable Decode.int))
        (Decode.oneOf
            [ Decode.map Err (Decode.field "error" Decode.string)
            , Decode.map Ok
                (Decode.map4 MetadataPayload
                    (Decode.field "body" Decode.string)
                    (Decode.field "authorizationEndpoint" Decode.string)
                    (Decode.field "tokenEndpoint" Decode.string)
                    (Decode.field "scopesSupported" Decode.string)
                )
            ]
        )


authResultDecoder : Decode.Decoder AuthResult
authResultDecoder =
    Decode.map2 AuthResult
        (Decode.field "configId" Decode.int)
        (Decode.oneOf
            [ Decode.map AuthFailure (Decode.field "error" Decode.string)
            , Decode.map AuthSuccess
                (Decode.map3 AuthSuccessData
                    (Decode.field "statusCode" Decode.int)
                    (Decode.field "headers" (Decode.list headerDecoder))
                    (Decode.field "body" Decode.string)
                )
            ]
        )


headerDecoder : Decode.Decoder ( String, String )
headerDecoder =
    Decode.map2 Tuple.pair
        (Decode.index 0 Decode.string)
        (Decode.index 1 Decode.string)


andMap : Decode.Decoder a -> Decode.Decoder (a -> b) -> Decode.Decoder b
andMap =
    Decode.map2 (|>)



-- ENCODING


{-| Encode the persistable fields of a config as the payload expected by the
create/update Tauri commands. Works for any record with these fields, so the
form module's output can be encoded with the same function.
-}
encodeConfigFields :
    { r
        | name : String
        , issuerUrl : String
        , authorizationUrl : String
        , tokenUrl : String
        , clientId : String
        , clientSecret : String
        , scopes : String
        , grantType : String
        , extraParams : Dict String String
        , disabledParams : Dict String Bool
        , disabledTokenParams : Dict String Bool
        , scopesSupported : String
        , disabledRefreshParams : Dict String Bool
    }
    -> List ( String, Encode.Value )
encodeConfigFields c =
    [ ( "name", Encode.string c.name )
    , ( "issuerUrl", Encode.string c.issuerUrl )
    , ( "authorizationUrl", Encode.string c.authorizationUrl )
    , ( "tokenUrl", Encode.string c.tokenUrl )
    , ( "clientId", Encode.string c.clientId )
    , ( "clientSecret", Encode.string c.clientSecret )
    , ( "scopes", Encode.string c.scopes )
    , ( "grantType", Encode.string c.grantType )
    , ( "extraParams", Encode.string (encodeExtraParams c.extraParams) )
    , ( "disabledParams", Encode.string (encodeDisabledParams c.disabledParams) )
    , ( "disabledTokenParams", Encode.string (encodeDisabledParams c.disabledTokenParams) )
    , ( "scopesSupported", Encode.string c.scopesSupported )
    , ( "disabledRefreshParams", Encode.string (encodeDisabledParams c.disabledRefreshParams) )
    ]


encodeExtraParams : Dict String String -> String
encodeExtraParams params =
    -- The backend treats the empty string as "no extra params"
    if Dict.isEmpty params then
        ""

    else
        Encode.encode 0 (Encode.dict identity Encode.string params)


encodeDisabledParams : Dict String Bool -> String
encodeDisabledParams params =
    Encode.encode 0 (Encode.dict identity Encode.bool params)



-- DISABLED PARAMS HELPERS


isParamDisabled : String -> Dict String Bool -> Bool
isParamDisabled paramName disabledParams =
    Dict.get paramName disabledParams == Just True


toggleDisabled : String -> Dict String Bool -> Dict String Bool
toggleDisabled paramName disabledParams =
    if Dict.member paramName disabledParams then
        Dict.remove paramName disabledParams

    else
        Dict.insert paramName True disabledParams



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
            (List.map (viewConfigCard callbackUrl model) model.configs)


viewConfigCard : String -> Model -> ClientConfig -> Html Msg
viewConfigCard callbackUrl model config =
    let
        state =
            Dict.get config.id model.authStates |> Maybe.withDefault Idle

        isExpanded =
            Set.member config.id model.expandedResults

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
            , if Dict.isEmpty config.extraParams then
                text ""

              else
                viewDetail "Extra Params" (extraParamsSummary config.extraParams)
            , if String.isEmpty config.scopesSupported then
                text ""

              else
                viewDetail "Scopes Supported" config.scopesSupported
            ]
        , if config.grantType == "authorization_code" then
            div []
                [ viewAuthorizeParamToggles model.expandedParams config
                , viewTokenParamToggles model.expandedTokenParams config
                ]

          else
            viewParamToggles model.expandedParams config
        , if not (String.isEmpty config.refreshToken) then
            viewRefreshParamToggles model.expandedRefreshParams config

          else
            text ""
        , viewMetadataSection model.expandedMetadata model.metadataStates config
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


extraParamsSummary : Dict String String -> String
extraParamsSummary params =
    Dict.toList params
        |> List.map (\( k, v ) -> k ++ "=" ++ v)
        |> String.join ", "


viewMetadataSection : Set Int -> Dict Int MetadataState -> ClientConfig -> Html Msg
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


viewParamToggles : Set Int -> ClientConfig -> Html Msg
viewParamToggles expandedParams config =
    let
        standardParams =
            [ "client_id", "client_secret", "scope" ]

        allParams =
            standardParams ++ Dict.keys config.extraParams
    in
    viewToggleSection
        { sectionTitle = "Send Parameters"
        , isExpanded = Set.member config.id expandedParams
        , toggleSectionMsg = ToggleParamsSection config.id
        , toggleParamMsg = ToggleParam
        , configId = config.id
        , disabledParams = config.disabledParams
        , params = allParams
        }


viewAuthorizeParamToggles : Set Int -> ClientConfig -> Html Msg
viewAuthorizeParamToggles expandedParams config =
    let
        authorizeParams =
            [ "response_type", "client_id", "redirect_uri", "state", "scope", "code_challenge", "code_challenge_method" ]
    in
    viewToggleSection
        { sectionTitle = "Authorize Parameters"
        , isExpanded = Set.member config.id expandedParams
        , toggleSectionMsg = ToggleParamsSection config.id
        , toggleParamMsg = ToggleParam
        , configId = config.id
        , disabledParams = config.disabledParams
        , params = authorizeParams ++ Dict.keys config.extraParams
        }


viewTokenParamToggles : Set Int -> ClientConfig -> Html Msg
viewTokenParamToggles expandedTokenParams config =
    let
        tokenParams =
            [ "client_id", "client_secret", "redirect_uri", "code_verifier" ]
    in
    viewToggleSection
        { sectionTitle = "Token Parameters"
        , isExpanded = Set.member config.id expandedTokenParams
        , toggleSectionMsg = ToggleTokenParamsSection config.id
        , toggleParamMsg = ToggleTokenParam
        , configId = config.id
        , disabledParams = config.disabledTokenParams
        , params = tokenParams ++ Dict.keys config.extraParams
        }


viewRefreshParamToggles : Set Int -> ClientConfig -> Html Msg
viewRefreshParamToggles expandedRefreshParams config =
    viewToggleSection
        { sectionTitle = "Refresh Token Parameters"
        , isExpanded = Set.member config.id expandedRefreshParams
        , toggleSectionMsg = ToggleRefreshParamsSection config.id
        , toggleParamMsg = ToggleRefreshParam
        , configId = config.id
        , disabledParams = config.disabledRefreshParams
        , params = [ "grant_type", "refresh_token", "client_id", "client_secret", "scope" ]
        }


viewToggleSection :
    { sectionTitle : String
    , isExpanded : Bool
    , toggleSectionMsg : Msg
    , toggleParamMsg : Int -> String -> Msg
    , configId : Int
    , disabledParams : Dict String Bool
    , params : List String
    }
    -> Html Msg
viewToggleSection cfg =
    let
        chevron =
            if cfg.isExpanded then
                "\u{25BC}"

            else
                "\u{25B6}"
    in
    div [ class "param-toggles" ]
        [ span [ class "expandable-toggle", onClick cfg.toggleSectionMsg ]
            [ text (chevron ++ " " ++ cfg.sectionTitle) ]
        , if cfg.isExpanded then
            div [ class "param-toggle-list" ]
                (List.map (viewParamToggle cfg.toggleParamMsg cfg.configId cfg.disabledParams) cfg.params)

          else
            text ""
        ]


viewParamToggle : (Int -> String -> Msg) -> Int -> Dict String Bool -> String -> Html Msg
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
