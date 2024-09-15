#include <iostream>
#include <WinSock2.h>
#include <ws2tcpip.h>  // For IPv6-related structures

#define PORT 9999  // Valid port number

struct sockaddr_in6 srv;  // Use sockaddr_in6 for IPv6

#pragma comment(lib, "Ws2_32.lib")

int main() {
    WSAData ws;

    // Initialize Winsock
    int err = WSAStartup(MAKEWORD(2, 2), &ws);
    if (err == 0) {
        std::cout << "Successfully initialized socket library!" << std::endl;
    } else {
        std::cout << "Unsuccessfully initialized the socket API" << std::endl;
        return 1;
    }

    // Get the host name
    char sHostName[32];
    err = gethostname(sHostName, 32);
    if (err == 0) {
        std::cout << "Hostname: " << sHostName << std::endl;
    } else {
        std::cout << "Hostname unavailable" << std::endl;
    }

    // Create a listening socket (IPv6)
    int listeningSocket = socket(AF_INET6, SOCK_STREAM, IPPROTO_TCP);
    if (listeningSocket < 0) {
        std::cout << "The socket has failed to open." << std::endl;
        return 1;
    } else {
        std::cout << "The socket has opened successfully." << std::endl;
    }

    // Prepare the sockaddr_in6 structure for binding
    srv.sin6_family = AF_INET6;
    srv.sin6_port = htons(PORT);  // Convert port to network byte order
    srv.sin6_addr = in6addr_any;  // IPv6 equivalent of INADDR_ANY

    // Bind the socket to the port
    err = bind(listeningSocket, (struct sockaddr*)&srv, sizeof(srv));
    if (err < 0) {
        std::cout << "Failed to bind to local port" << std::endl;
        return 1;
    } else {
        std::cout << "Bind to local port successfully" << std::endl;
    }

    //listening for the connection 
    err = listen(listeningSocket, 5); //socket and how many clients can wait 
    if (err < 0){
        std::cout << "Backlog full" << std::endl; 
    } else {
        std::cout << "Listening..." << std::endl; 
    }

    //accept the connection (only a single client)
    struct sockaddr_in client; 
    int length = sizeof(client);

    //will become the communication socket 
    int clientSocket = accept(listeningSocket, (struct sockaddr*)&client, &length);  //client, client sockaadr

    if (clientSocket < 0){
        std::cout << "Failed to accept connection." << std::endl; 
    } else {
        std::cout << "Connection accepted." << std::endl; 
        std::cout << "IP Address: " << client.sin_addr.s_addr; 
    }
    
    return 0;
}